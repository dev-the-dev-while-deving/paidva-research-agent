//! Full-screen chat (ratatui): transcript on the left, a tabbed run panel on the right, one input line.
//! The agent runs in a tokio task; its steps and notes arrive through `ui::SINK`.
//! The answer, sources and code views are the same ANSI strings the line mode prints, converted with ansi-to-tui.
use crate::agent::{Agent, Outcome};
use crate::answer::{Claim, Evidence, Kind, Tag};
use crate::llm::CallLog;
use crate::{Last, config, ui};
use ansi_to_tui::IntoText;
use anyhow::Result;
use console::measure_text_width as width_of;
use ratatui::crossterm::event::{self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const AMBER: Color = Color::Indexed(ui::AMBER);
const OK: Color = Color::Indexed(114);
const BAD: Color = Color::Indexed(203);
const BAR: Color = Color::Indexed(236);
const SPIN: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const TABS: [&str; 5] = ["Steps", "Claims", "Sources", "Code", "Reasoning"];
/// Below this width the transcript and the panel share one pane (Tab flips between them).
const WIDE: u16 = 110;

/// The logo: a galloping horse in braille dots (51×40 cells).
// ponytail: converted from the reference image the owner supplied; re-convert from an owned or licensed image before release.
const HORSE: &str = include_str!("horse.txt");

fn dim() -> Style { Style::new().add_modifier(Modifier::DIM) }
fn bold() -> Style { Style::new().add_modifier(Modifier::BOLD) }
fn amber() -> Style { Style::new().fg(AMBER) }
fn ansi(s: &str) -> Text<'static> { s.into_text().unwrap_or_else(|_| Text::raw(s.to_string())) }

enum Entry { User(String), Answer(usize), Ask(String), Note(String), Error(String) }

#[derive(PartialEq, Clone, Copy, Debug)]
enum Focus { Input, Transcript, Panel }

struct Step { name: String, detail: String, secs: f64, note: bool }

struct App {
    agent: Option<Arc<Agent>>,
    files: Vec<PathBuf>,
    /// (question, direct answer) pairs the agent sees as conversation history.
    history: Vec<(String, String)>,
    log: Vec<Entry>,
    answers: Vec<(Last, ui::Stats)>,
    steps: Vec<Step>,
    /// Model calls of the last finished run, for the Reasoning tab.
    calls: Vec<CallLog>,
    run: Option<(tokio::task::JoinHandle<Result<Outcome>>, Instant, String)>,
    rx: mpsc::Receiver<ui::Event>,
    input: String,
    /// Byte offset into `input`.
    cursor: usize,
    past: Vec<String>,
    past_i: usize,
    focus: Focus,
    tab: usize,
    wide: bool,
    show_panel: bool,
    /// Transcript lines scrolled up from the bottom.
    scroll: usize,
    pscroll: usize,
    sel: usize,
    risky: bool,
    thinking: bool,
    quit: bool,
}

/// Run the full-screen chat until the user quits.
pub async fn run(files: Vec<PathBuf>) -> Result<()> {
    let agent = Arc::new(Agent::new()?);
    let (tx, rx) = mpsc::channel();
    *ui::SINK.lock().unwrap() = Some(tx);
    let mut app = App::new(Some(agent), files, rx);
    let mut term = ratatui::init(); // raw mode, alternate screen, and a panic hook that restores the terminal
    let res = app.main_loop(&mut term).await;
    ratatui::restore();
    *ui::SINK.lock().unwrap() = None;
    app.save_past();
    res
}

fn history_path() -> PathBuf { config::dir().join("history.txt") }

impl App {
    fn new(agent: Option<Arc<Agent>>, files: Vec<PathBuf>, rx: mpsc::Receiver<ui::Event>) -> Self {
        // Shares rustyline's history file: one entry per line after its "#V2" header.
        // ponytail: rustyline escapes backslashes in that file; we read and write lines raw.
        let past: Vec<String> = std::fs::read_to_string(history_path()).unwrap_or_default()
            .lines().filter(|l| !l.is_empty() && *l != "#V2").map(str::to_string).collect();
        Self {
            agent, files, history: vec![], log: vec![], answers: vec![], steps: vec![], calls: vec![], run: None, rx,
            input: String::new(), cursor: 0, past_i: past.len(), past,
            focus: Focus::Input, tab: 0, wide: true, show_panel: false,
            scroll: 0, pscroll: 0, sel: 0, risky: true, thinking: true, quit: false,
        }
    }

    fn save_past(&self) {
        let keep = &self.past[self.past.len().saturating_sub(1000)..];
        let _ = std::fs::write(history_path(), format!("#V2\n{}\n", keep.join("\n")));
    }

    fn last(&self) -> Option<&Last> { self.answers.last().map(|(l, _)| l) }

    async fn main_loop(&mut self, term: &mut DefaultTerminal) -> Result<()> {
        while !self.quit {
            self.pump().await;
            term.draw(|f| self.draw(f))?;
            // ponytail: polls the keyboard every 80 ms on a blocking-allowed worker; switch to crossterm's EventStream if it ever shows in a profile.
            let ev = tokio::task::block_in_place(|| -> std::io::Result<Option<TermEvent>> {
                if event::poll(Duration::from_millis(80))? { Ok(Some(event::read()?)) } else { Ok(None) }
            })?;
            if let Some(TermEvent::Key(k)) = ev && k.kind == KeyEventKind::Press { self.key(k, term).await?; }
        }
        if let Some((h, ..)) = self.run.take() { h.abort(); }
        Ok(())
    }

    /// Take in progress events and a finished run.
    async fn pump(&mut self) {
        let secs = self.run.as_ref().map(|r| r.1.elapsed().as_secs_f64()).unwrap_or(0.0);
        while let Ok(e) = self.rx.try_recv() {
            self.steps.push(match e {
                ui::Event::Step(name, detail) => Step { name, detail, secs, note: false },
                ui::Event::Note(detail) => Step { name: String::new(), detail, secs, note: true },
            });
        }
        if !self.run.as_ref().is_some_and(|r| r.0.is_finished()) { return; }
        let (h, t0, q) = self.run.take().unwrap();
        self.calls = self.agent.as_ref().map(|a| a.llm.calls.lock().unwrap().clone()).unwrap_or_default();
        let st = ui::Stats { calls: self.calls.len(), tokens: self.calls.iter().map(|c| c.tokens).sum(), secs: t0.elapsed().as_secs_f64() };
        match h.await {
            Ok(Ok(Outcome::Answered { answer, evidence, report })) => {
                self.history.push((q, answer.direct_answer.clone()));
                self.answers.push(((answer, evidence, report), st));
                self.log.push(Entry::Answer(self.answers.len() - 1));
                (self.tab, self.sel, self.pscroll) = (1, 0, 0);
            }
            Ok(Ok(Outcome::Ask(c))) => self.log.push(Entry::Ask(c)),
            Ok(Err(e)) => self.log.push(Entry::Error(format!("{e:#}"))),
            Err(_) => {} // aborted: already noted
        }
        self.scroll = 0;
    }

    fn start(&mut self, q: String) {
        let Some(agent) = self.agent.clone() else { return };
        self.log.push(Entry::User(q.clone()));
        self.steps.clear();
        (self.tab, self.scroll, self.pscroll) = (0, 0, 0);
        let (files, hist, q2) = (self.files.clone(), self.history.clone(), q.clone());
        let h = tokio::spawn(async move { agent.handle(&q2, &files, &hist).await });
        self.run = Some((h, Instant::now(), q));
    }

    fn show_tab(&mut self, t: usize) {
        (self.tab, self.pscroll) = (t, 0);
        self.focus = Focus::Panel;
        if !self.wide { self.show_panel = true; }
    }

    // ───────────────────────── input ─────────────────────────

    async fn key(&mut self, k: KeyEvent, term: &mut DefaultTerminal) -> Result<()> {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Char('c') if ctrl => match self.run.take() {
                Some((h, ..)) => { h.abort(); self.log.push(Entry::Error("stopped".into())); }
                None if self.input.is_empty() => self.quit = true,
                None => { self.input.clear(); self.cursor = 0; }
            },
            KeyCode::Char('d') if ctrl => if self.input.is_empty() { self.quit = true; },
            KeyCode::Tab | KeyCode::BackTab if !self.wide => {
                self.show_panel = !self.show_panel;
                self.focus = if self.show_panel { Focus::Panel } else { Focus::Input };
            }
            KeyCode::Tab => self.focus = match self.focus { Focus::Input => Focus::Transcript, Focus::Transcript => Focus::Panel, Focus::Panel => Focus::Input },
            KeyCode::BackTab => self.focus = match self.focus { Focus::Input => Focus::Panel, Focus::Panel => Focus::Transcript, Focus::Transcript => Focus::Input },
            KeyCode::Esc => { self.focus = Focus::Input; self.show_panel = false; }
            KeyCode::PageUp if self.focus == Focus::Panel => self.pscroll = self.pscroll.saturating_sub(10),
            KeyCode::PageDown if self.focus == Focus::Panel => self.pscroll += 10,
            KeyCode::PageUp => self.scroll += 10,
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(10),
            _ => match self.focus {
                Focus::Input => self.edit(k, term).await?,
                Focus::Transcript => match k.code {
                    KeyCode::Up | KeyCode::Char('k') => self.scroll += 1,
                    KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_sub(1),
                    KeyCode::Char('g') => self.scroll = usize::MAX,
                    KeyCode::Char('G') => self.scroll = 0,
                    KeyCode::Char('?') => self.log.push(Entry::Note(help())),
                    KeyCode::Char(c @ '1'..='5') => self.show_tab(c as usize - '1' as usize),
                    _ => { self.focus = Focus::Input; self.edit(k, term).await?; }
                },
                Focus::Panel => match k.code {
                    KeyCode::Char(c @ '1'..='5') => self.show_tab(c as usize - '1' as usize),
                    KeyCode::Up | KeyCode::Char('k') if self.tab == 1 => self.sel = self.sel.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') if self.tab == 1 => self.sel += 1,
                    KeyCode::Up | KeyCode::Char('k') => self.pscroll = self.pscroll.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => self.pscroll += 1,
                    KeyCode::Char('r') => { self.risky = !self.risky; self.sel = 0; }
                    KeyCode::Char('t') => self.thinking = !self.thinking,
                    KeyCode::Char('?') => self.log.push(Entry::Note(help())),
                    KeyCode::Enter => self.open_selected(),
                    _ => {}
                },
            },
        }
        Ok(())
    }

    async fn edit(&mut self, k: KeyEvent, term: &mut DefaultTerminal) -> Result<()> {
        let prev = |s: &str, i: usize| s[..i].chars().next_back().map_or(i, |c| i - c.len_utf8());
        let next = |s: &str, i: usize| s[i..].chars().next().map_or(i, |c| i + c.len_utf8());
        match k.code {
            KeyCode::Enter => self.submit(term).await?,
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => { self.input.insert(self.cursor, c); self.cursor += c.len_utf8(); }
            KeyCode::Backspace if self.cursor > 0 => { let p = prev(&self.input, self.cursor); self.input.replace_range(p..self.cursor, ""); self.cursor = p; }
            KeyCode::Delete if self.cursor < self.input.len() => { let n = next(&self.input, self.cursor); self.input.replace_range(self.cursor..n, ""); }
            KeyCode::Left => self.cursor = prev(&self.input, self.cursor),
            KeyCode::Right => self.cursor = next(&self.input, self.cursor),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.len(),
            KeyCode::Up if self.past_i > 0 => { self.past_i -= 1; self.input = self.past[self.past_i].clone(); self.cursor = self.input.len(); }
            KeyCode::Down if self.past_i < self.past.len() => {
                self.past_i += 1;
                self.input = self.past.get(self.past_i).cloned().unwrap_or_default();
                self.cursor = self.input.len();
            }
            _ => {}
        }
        Ok(())
    }

    async fn submit(&mut self, term: &mut DefaultTerminal) -> Result<()> {
        let q = self.input.trim().to_string();
        (self.input, self.cursor) = (String::new(), 0);
        if q.is_empty() { return Ok(()); }
        if self.past.last() != Some(&q) { self.past.push(q.clone()); }
        self.past_i = self.past.len();
        let (cmd, arg) = q.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((&q, ""));
        let note = |s: &str| Entry::Note(s.to_string());
        match cmd {
            "/quit" | "/exit" | "/q" => self.quit = true,
            "/help" | "/?" => self.log.push(Entry::Note(help())),
            "/why" => self.show_tab(4),
            "/sources" => self.show_tab(2),
            "/code" => self.show_tab(3),
            "/json" => match self.last() {
                Some((a, ev, r)) => { let j = serde_json::to_string_pretty(&serde_json::json!({"answer": a, "evidence": ev, "report": r}))?; self.log.push(Entry::Note(j)); }
                None => self.log.push(note("nothing yet: ask a question first")),
            },
            "/file" if arg.is_empty() => self.log.push(Entry::Note(if self.files.is_empty() { "no files attached".into() } else { format!("attached: {}", crate::names(&self.files)) })),
            "/file" => {
                let p = PathBuf::from(crate::shellish(arg));
                if p.is_file() { self.log.push(Entry::Note(format!("✓ attached {}", p.display()))); self.files.push(p); }
                else { self.log.push(Entry::Error(format!("no such file: {}", p.display()))); }
            }
            "/drop" => { self.files.clear(); self.log.push(note("✓ files detached")); }
            "/clear" => { self.log.clear(); self.history.clear(); self.answers.clear(); self.steps.clear(); self.calls.clear(); }
            "/settings" | "/doctor" => {
                // Both are line-based prompts: leave the full screen while they run.
                ratatui::restore();
                *ui::SINK.lock().unwrap() = None;
                if cmd == "/settings" { crate::setup::settings().await?; } else {
                    println!();
                    crate::setup::doctor(true).await;
                    println!("\n  press Enter to go back");
                    let _ = std::io::stdin().read_line(&mut String::new());
                }
                let (tx, rx) = mpsc::channel();
                (*ui::SINK.lock().unwrap(), self.rx) = (Some(tx), rx);
                *term = ratatui::init();
            }
            c if c.starts_with('/') => self.log.push(Entry::Error(format!("unknown command {c} · /help lists them"))),
            _ if self.run.is_some() => self.log.push(note("still working on the last question: Ctrl-C stops it")),
            _ => self.start(q),
        }
        self.scroll = 0;
        Ok(())
    }

    fn open_selected(&self) {
        let Some((a, ev, _)) = self.last() else { return };
        let order = claim_order(&a.claims, self.risky);
        let Some(c) = order.get(self.sel.min(order.len().saturating_sub(1))).map(|&i| &a.claims[i]) else { return };
        let url = c.evidence.iter().filter_map(|id| ev.iter().find(|e| &e.id == id)).map(|e| e.url.as_str())
            .find(|u| u.starts_with("https://") || u.starts_with("http://"));
        if let Some(u) = url {
            let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
            let _ = std::process::Command::new(opener).arg(u).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
        }
    }

    // ───────────────────────── drawing ─────────────────────────

    fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        if area.height < 12 || area.width < 40 {
            f.render_widget(Paragraph::new("Paidva: make the window bigger").style(amber()), area);
            return;
        }
        self.wide = area.width >= WIDE;
        let home = self.log.is_empty() && self.run.is_none();
        let [head, body, input, foot] = Layout::vertical([Constraint::Length(if home { 0 } else { 1 }), Constraint::Min(0), Constraint::Length(2), Constraint::Length(1)]).areas(area);
        if !home { self.header(f, head); }
        if home { self.home(f, body); }
        else if self.wide {
            let [l, r] = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).areas(body);
            self.transcript(f, l);
            self.panel(f, r);
        } else if self.show_panel { self.panel(f, body); } else { self.transcript(f, body); }
        self.input_line(f, input, home);
        self.footer(f, foot);
    }

    fn header(&self, f: &mut Frame, a: Rect) {
        let c = config::get();
        let short = |v: &[String]| v.first().map(|m| m.rsplit('/').next().unwrap_or(m).trim_end_matches(":free").to_string()).unwrap_or_default();
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(" ◆ Paidva  ", amber().add_modifier(Modifier::BOLD)),
            Span::styled(format!("main {} · fast {} · sandbox {}s", short(&c.models.main), short(&c.models.fast), c.sandbox.timeout_secs), dim()),
        ])), a);
        if !self.files.is_empty() {
            f.render_widget(Paragraph::new(format!("{} ", crate::names(&self.files))).alignment(Alignment::Right), a);
        }
    }

    fn home(&self, f: &mut Frame, a: Rect) {
        let info = home_info(&self.files);
        let horse: Vec<&str> = HORSE.lines().collect();
        let hw = horse.iter().map(|l| width_of(l)).max().unwrap_or(0) as u16;
        let iw = 56u16;
        if a.width >= hw + iw + 10 && a.height as usize >= horse.len() + 2 {
            let x = a.x + (a.width - hw - iw - 8) / 2;
            let y = a.y + (a.height - horse.len() as u16) / 2;
            f.render_widget(Paragraph::new(HORSE).style(amber()), Rect::new(x, y, hw, horse.len() as u16));
            let ih = info.len() as u16;
            f.render_widget(Paragraph::new(info), Rect::new(x + hw + 8, a.y + a.height.saturating_sub(ih) / 2, iw, ih.min(a.height)));
        } else {
            // Too small for the horse: the one-line logo instead.
            let ih = info.len() as u16;
            let w = iw.min(a.width);
            f.render_widget(Paragraph::new(info), Rect::new(a.x + (a.width - w) / 2, a.y + a.height.saturating_sub(ih) / 2, w, ih.min(a.height)));
        }
    }

    fn transcript(&mut self, f: &mut Frame, a: Rect) {
        let border = if self.wide { Borders::RIGHT } else { Borders::NONE };
        let block = Block::new().borders(border).border_style(if self.focus == Focus::Transcript { amber() } else { dim() });
        let inner = block.inner(a);
        f.render_widget(block, a);
        let inner = Rect { x: inner.x + 1, width: inner.width.saturating_sub(2), ..inner };
        let w = (inner.width as usize).clamp(30, 100);
        let lines = self.transcript_lines(w);
        // ponytail: re-renders every entry each frame; cache per (entry, width) if long chats get sluggish.
        let max = lines.len().saturating_sub(inner.height as usize);
        self.scroll = self.scroll.min(max);
        f.render_widget(Paragraph::new(lines).scroll(((max - self.scroll) as u16, 0)), inner);
    }

    fn transcript_lines(&self, w: usize) -> Vec<Line<'static>> {
        let mut out: Vec<Line> = vec![];
        let wrapped = |s: &str, first: Span<'static>, style: Style| -> Vec<Line<'static>> {
            ui::wrap(s, w.saturating_sub(2), 2).lines().enumerate()
                .map(|(i, l)| Line::from(vec![if i == 0 { first.clone() } else { Span::raw("") }, Span::styled(l.to_string(), style)]))
                .collect()
        };
        for e in &self.log {
            match e {
                Entry::User(q) => { out.push(Line::raw("")); out.extend(wrapped(q, Span::styled("› ", amber()), bold())); }
                Entry::Answer(i) => {
                    let ((a, ev, r), st) = &self.answers[*i];
                    out.extend(ansi(&ui::answer(a, ev, r, st, w)).lines);
                    for e in &r.errors { out.extend(wrapped(e, Span::styled("  note: ", dim()), dim())); }
                }
                Entry::Ask(c) => { out.push(Line::raw("")); out.extend(wrapped(c, Span::styled("  ? ", amber()), Style::new())); }
                Entry::Note(n) => out.extend(wrapped(n, Span::raw("  "), dim())),
                Entry::Error(e) => out.extend(wrapped(e, Span::styled("  ✗ ", Style::new().fg(BAD)), Style::new().fg(BAD))),
            }
        }
        if let Some((_, t0, _)) = &self.run {
            let last = self.steps.iter().rev().find(|s| !s.note).map(|s| format!(" · {} done", s.name)).unwrap_or_default();
            out.push(Line::raw(""));
            out.push(Line::from(vec![Span::styled(format!("  {} ", spin(t0)), amber()), Span::styled(format!("working{last} · {:.1}s", t0.elapsed().as_secs_f64()), dim())]));
        }
        out
    }

    fn panel(&mut self, f: &mut Frame, a: Rect) {
        let focused = self.focus == Focus::Panel;
        let [tabs, body] = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(a);
        let mut spans = vec![Span::raw(" ")];
        for (i, t) in TABS.iter().enumerate() {
            let s = format!(" {} {t} ", i + 1);
            spans.push(if i == self.tab { Span::styled(s, Style::new().bg(if focused { AMBER } else { Color::Indexed(244) }).fg(Color::Black).add_modifier(Modifier::BOLD)) } else { Span::styled(s, dim()) });
        }
        f.render_widget(Paragraph::new(Line::from(spans)).block(Block::new().borders(Borders::BOTTOM).border_style(dim())), tabs);
        let body = Rect { x: body.x + 1, width: body.width.saturating_sub(2), ..body };
        let pw = (body.width as usize).max(20);
        let text: Text<'static> = match self.tab {
            0 => Text::from(self.steps_lines()),
            1 => Text::from(self.claim_lines(pw)),
            2 => self.last().map(|(_, ev, _)| ansi(&ui::sources(ev, pw.min(100)))).unwrap_or_else(|| Text::styled("  no answer yet", dim())),
            3 => self.last().map(|(_, ev, _)| ansi(&ui::code(ev, pw.min(100)))).unwrap_or_else(|| Text::styled("  no answer yet", dim())),
            _ => Text::from(self.reasoning_lines(pw)),
        };
        f.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }).scroll((self.pscroll as u16, 0)), body);
    }

    fn steps_lines(&self) -> Vec<Line<'static>> {
        let mut out = vec![];
        for s in &self.steps {
            out.push(if s.note { Line::styled(format!("  … {}", s.detail), dim()) } else {
                Line::from(vec![Span::styled("  ◆ ", amber()), Span::styled(format!("{:<11}", s.name), bold()), Span::raw(format!("{} ", s.detail)), Span::styled(format!("{:.1}s", s.secs), dim())])
            });
        }
        match &self.run {
            Some((_, t0, _)) => out.push(Line::from(vec![Span::styled(format!("  {} ", spin(t0)), amber()), Span::styled(format!("working… {:.1}s", t0.elapsed().as_secs_f64()), dim())])),
            None if out.is_empty() => out.push(Line::styled("  steps show here while a question runs", dim())),
            None => {}
        }
        out
    }

    fn claim_lines(&mut self, pw: usize) -> Vec<Line<'static>> {
        let Some((a, ev, _)) = self.answers.last().map(|(l, _)| l) else { return vec![Line::styled("  no answer yet", dim())] };
        if a.claims.is_empty() { return vec![Line::styled("  this answer has no separate claims", dim())]; }
        let order = claim_order(&a.claims, self.risky);
        self.sel = self.sel.min(order.len() - 1);
        let mut out = vec![Line::from(vec![Span::styled("  r ", amber()), Span::styled(format!("risky first {}", if self.risky { "on" } else { "off" }), dim())]), Line::raw("")];
        for (row, &i) in order.iter().enumerate() {
            let c = &a.claims[i];
            let ids = if c.evidence.is_empty() { "—".to_string() } else { c.evidence.join(" ") };
            let room = pw.saturating_sub(18 + ids.len());
            let text: String = if c.text.chars().count() > room { c.text.chars().take(room.saturating_sub(1)).collect::<String>() + "…" } else { c.text.clone() };
            let picked = row == self.sel;
            let mut spans = vec![Span::styled(if picked { "▸ " } else { "  " }, amber())];
            spans.extend(ansi(&ui::chip(c.tag)).lines.into_iter().next().map(|l| l.spans).unwrap_or_default());
            spans.push(Span::styled(format!(" {text} "), if picked { bold() } else { Style::new() }));
            spans.push(Span::styled(ids, dim()));
            out.push(Line::from(spans));
        }
        out.push(Line::styled("─".repeat(pw), dim()));
        out.extend(evidence_lines(&a.claims[order[self.sel]], ev));
        out
    }

    fn reasoning_lines(&self, pw: usize) -> Vec<Line<'static>> {
        let wrap = |s: &str, ind: usize| ui::wrap(s, pw, ind);
        if self.steps.is_empty() && self.calls.is_empty() { return vec![Line::styled("  ask something: how the answer was reached shows here", dim())]; }
        let mut out = vec![Line::styled("  how the answer was reached", dim()), Line::raw("")];
        for (i, s) in self.steps.iter().filter(|s| !s.note).enumerate() {
            out.extend(rows(vec![Span::styled(format!("  {:>2} ", i + 1), amber()), Span::styled(format!("{:<13}", s.name), bold())], wrap(&s.detail, 18), Style::new()));
        }
        if self.run.is_none() && let Some((_, _, r)) = self.last() {
            for issue in &r.first_draft_issues { out.extend(rows(vec![Span::styled("   ✗ ", Style::new().fg(BAD))], wrap(&format!("verify caught: {issue}"), 5), Style::new())); }
            if r.relabelled + r.dropped > 0 { out.push(Line::styled(format!("     {} claim(s) relabelled, {} dropped", r.relabelled, r.dropped), dim())); }
            if let Some(why) = &r.answer_revised { out.push(Line::styled(format!("     answer rewritten: {why}"), dim())); }
            out.extend(rows(vec![Span::styled("   = ", Style::new().fg(OK))], wrap(&format!("confidence {:.2} after {} fix round(s) (uncalibrated)", r.confidence, r.fix_rounds), 5), Style::new()));
        }
        out.push(Line::raw(""));
        out.push(Line::from(vec![Span::styled("  model thinking", bold()), Span::styled(format!(" · t {}", if self.thinking { "hides it" } else { "shows it" }), dim())]));
        if !self.thinking { return out; }
        out.extend(rows(vec![Span::raw("  ")], wrap("what the model wrote before each reply; it does not always match what it then did", 2), dim()));
        if self.calls.is_empty() { out.push(Line::styled("  (shows when the run finishes)", dim())); }
        for c in &self.calls {
            out.push(Line::raw(""));
            out.push(Line::from(vec![Span::styled("  ▸ ", amber()), Span::styled(c.step.clone(), bold()), Span::styled(format!(" · {}{}", c.model.rsplit('/').next().unwrap_or(&c.model), if c.cached { " · cached" } else { "" }), dim())]));
            if c.reasoning.trim().is_empty() { out.push(Line::styled("    (no thinking returned)", dim())); }
            for para in c.reasoning.trim().lines().filter(|l| !l.trim().is_empty()) {
                out.extend(rows(vec![Span::raw("    ")], wrap(para, 4), dim().add_modifier(Modifier::ITALIC)));
            }
        }
        out
    }

    fn input_line(&self, f: &mut Frame, a: Rect, home: bool) {
        let block = Block::new().borders(Borders::TOP).border_style(if self.focus == Focus::Input { amber() } else { dim() });
        let inner = block.inner(a);
        f.render_widget(block, a);
        let room = (inner.width as usize).saturating_sub(4);
        // Show the tail of an input longer than the line.
        let before = width_of(&self.input[..self.cursor]);
        let mut skip = 0;
        while width_of(&self.input[skip..self.cursor]) > room { skip += self.input[skip..].chars().next().map_or(1, char::len_utf8); }
        let shown: String = self.input[skip..].to_string();
        let line = if self.input.is_empty() {
            Line::from(vec![Span::styled(" › ", amber()), Span::styled(if home { "Ask anything · /help" } else { "ask a follow-up, or /help" }, dim())])
        } else { Line::from(vec![Span::styled(" › ", amber()), Span::raw(shown)]) };
        f.render_widget(Paragraph::new(line), inner);
        if self.focus == Focus::Input {
            let col = before - width_of(&self.input[..skip]);
            f.set_cursor_position((inner.x + 3 + col as u16, inner.y));
        }
    }

    fn footer(&self, f: &mut Frame, a: Rect) {
        f.render_widget(Block::new().style(Style::new().bg(BAR)), a);
        let left = match (&self.run, self.answers.last()) {
            (Some((_, t0, _)), _) => {
                let (n, tok) = self.agent.as_ref().map(|ag| { let c = ag.llm.calls.lock().unwrap(); (c.len(), c.iter().map(|c| c.tokens).sum::<u64>()) }).unwrap_or_default();
                Line::from(vec![Span::styled(format!(" {} {:.1}s", spin(t0), t0.elapsed().as_secs_f64()), amber()), Span::styled(format!(" · {n} calls · {:.1}k tok", tok as f64 / 1000.0), dim())])
            }
            (None, Some((_, st))) if !self.log.is_empty() => Line::from(vec![Span::styled(format!(" ✓ done {:.1}s", st.secs), Style::new().fg(OK)), Span::styled(format!(" · {} calls · {:.1}k tok", st.calls, st.tokens as f64 / 1000.0), dim())]),
            _ => Line::styled(if self.files.is_empty() { " no files attached".to_string() } else { format!(" attached: {}", crate::names(&self.files)) }, dim()),
        };
        let hints = match self.focus {
            Focus::Input if self.wide => "Tab focus · PgUp PgDn scroll · Ctrl-C stop · Ctrl-D quit ",
            Focus::Input => "Tab panel · Ctrl-C stop · Ctrl-D quit ",
            Focus::Transcript => "↑↓ PgUp PgDn scroll · 1-5 panel · Esc input ",
            Focus::Panel => "1-5 tab · ↑↓ · r risky · t thinking · Enter open · Esc input ",
        };
        let fits = a.width as usize > left.width() + width_of(hints) + 2;
        f.render_widget(Paragraph::new(left), a);
        if fits { f.render_widget(Paragraph::new(Span::styled(hints, dim())).alignment(Alignment::Right), a); }
    }
}

/// `prefix` then the first line of pre-wrapped `text`; its later lines (already indented by `ui::wrap`) as their own lines.
fn rows(prefix: Vec<Span<'static>>, text: String, style: Style) -> Vec<Line<'static>> {
    let mut lines = text.lines();
    let mut first = prefix;
    first.push(Span::styled(lines.next().unwrap_or("").to_string(), style));
    std::iter::once(Line::from(first)).chain(lines.map(|l| Line::styled(l.to_string(), style))).collect()
}

fn spin(t0: &Instant) -> &'static str { SPIN[(t0.elapsed().as_millis() / 80) as usize % SPIN.len()] }

/// Claim indices in display order: riskiest tags first when `risky`, else as written.
fn claim_order(claims: &[Claim], risky: bool) -> Vec<usize> {
    let rank = |t: Tag| match t { Tag::Speculation => 0, Tag::Inferred => 1, Tag::Knowledge => 2, Tag::Sourced => 3, Tag::Observed => 4 };
    let mut idx: Vec<usize> = (0..claims.len()).collect();
    if risky { idx.sort_by_key(|&i| rank(claims[i].tag)); } // stable: ties keep their written order
    idx
}

/// The evidence a claim cites: code with its named RESULT lines highlighted, or the source text with the quote highlighted.
fn evidence_lines(c: &Claim, ev: &[Evidence]) -> Vec<Line<'static>> {
    let hi = Style::new().bg(Color::Indexed(58)).fg(Color::Indexed(223));
    let mut out = vec![];
    if c.evidence.is_empty() { out.push(Line::styled(format!("  no evidence cited (tagged {})", ui::tag_name(c.tag)), dim())); }
    let named = |l: &str| l.contains("RESULT") && c.results.iter().any(|n| l.contains(n.as_str()));
    for id in &c.evidence {
        out.push(Line::raw(""));
        let Some(e) = ev.iter().find(|e| &e.id == id) else { out.push(Line::styled(format!("  [{id}] not found in this answer's evidence"), Style::new().fg(BAD))); continue };
        let mut head = vec![Span::styled(format!("  [{}] ", e.id), amber()), Span::styled(e.title.clone(), bold())];
        if e.partial { head.push(Span::styled("  (abstract only)", dim())); }
        out.push(Line::from(head));
        if e.kind == Kind::Computation {
            for l in e.code.lines() { out.push(Line::styled(format!("  │ {l}"), if named(l) { hi } else { dim() })); }
            out.push(Line::styled("  └─ output", dim()));
            for l in e.text.lines().take(12) { out.push(Line::styled(format!("     {l}"), if named(l) { hi } else { dim() })); }
            continue;
        }
        let q = c.quote.trim();
        match (!q.is_empty()).then(|| e.text.find(q)).flatten() {
            Some(p) => {
                let from = e.text.floor_char_boundary(p.saturating_sub(240));
                let to = e.text.ceil_char_boundary((p + q.len() + 240).min(e.text.len()));
                out.push(Line::from(vec![
                    Span::styled(format!("  …{}", &e.text[from..p]), dim()),
                    Span::styled(q.to_string(), hi),
                    Span::styled(format!("{}…", &e.text[p + q.len()..to]), dim()),
                ]));
            }
            None => {
                out.push(Line::styled(format!("  {}", e.text.chars().take(400).collect::<String>()), dim()));
                if !q.is_empty() {
                    out.push(Line::styled("  ⚠ quote not found in the source text:", amber()));
                    out.push(Line::styled(format!("  “{q}”"), dim().add_modifier(Modifier::ITALIC)));
                }
            }
        }
        if !e.url.is_empty() { out.push(Line::styled(format!("  {}", e.url), dim().add_modifier(Modifier::UNDERLINED))); }
    }
    out
}

/// Wordmark, version and offline health lines for the home screen (no network: `/doctor` does the live checks).
fn home_info(files: &[PathBuf]) -> Vec<Line<'static>> {
    let c = config::get();
    let has = |k: &str| std::env::var(k).is_ok_and(|v| !v.trim().is_empty());
    let model = |label: &str, chain: &[String]| -> Line<'static> {
        let spec = chain.first().cloned().unwrap_or_default();
        let key = crate::llm::route(&spec).1;
        let name = spec.rsplit('/').next().unwrap_or(&spec).trim_end_matches(":free").to_string();
        if has(key) { row(true, label, name.clone()) } else { row(false, label, format!("{name}: {key} not set")) }
    };
    let py = crate::sandbox::python(config::dir().parent().unwrap_or(&config::dir())).exists();
    let mut out = vec![
        Line::styled("P A I D V A", amber().add_modifier(Modifier::BOLD)),
        Line::styled(format!("v{} · the research agent that shows its work", env!("CARGO_PKG_VERSION")), dim()),
        Line::raw(""),
        model("main", &c.models.main),
        model("fast", &c.models.fast),
        if py { row(true, "sandbox", format!("python · {}s limit", c.sandbox.timeout_secs)) } else { row(false, "sandbox", "not installed: run paidva setup".into()) },
        if has("S2_API_KEY") { row(true, "papers", "Semantic Scholar key set".into()) } else { row(false, "papers", "no S2_API_KEY: Semantic Scholar may 429".into()) },
    ];
    if !files.is_empty() { out.push(Line::raw("")); out.push(Line::styled(format!("attached: {}", crate::names(files)), dim())); }
    out
}

fn row(ok: bool, label: &str, text: String) -> Line<'static> {
    Line::from(vec![
        if ok { Span::styled("✓ ", Style::new().fg(OK)) } else { Span::styled("! ", amber()) },
        Span::styled(format!("{label:<9}"), dim()),
        Span::raw(text),
    ])
}

fn help() -> String {
    [
        "keys: Tab moves focus (input → transcript → panel) · Esc back to input · Ctrl-C stops a run · Ctrl-D quits",
        "panel: 1 Steps · 2 Claims · 3 Sources · 4 Code · 5 Reasoning · ↑↓ select or scroll · r risky first · t model thinking · Enter opens the claim's source",
        "commands: /file <path> · /file · /drop · /json · /clear · /settings · /doctor · /quit",
    ].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Report;
    use crate::answer::Answer;
    use ratatui::{Terminal, backend::TestBackend};

    fn claim(text: &str, tag: Tag, ev: &[&str]) -> Claim {
        Claim { text: text.into(), tag, evidence: ev.iter().map(|s| s.to_string()).collect(), quote: String::new(), results: vec!["diff".into()] }
    }

    fn app_with_answer() -> App {
        let (_tx, rx) = mpsc::channel();
        let mut app = App::new(None, vec![], rx);
        let a = Answer {
            direct_answer: "Yes, modestly: 41% vs 29%.".into(),
            claims: vec![claim("Response was 41% vs 29%.", Tag::Observed, &["C1"]), claim("Benefit may fade.", Tag::Speculation, &[])],
            bayes: vec![], unknowns: vec![], self_confidence: 0.7,
        };
        let ev = vec![Evidence { id: "C1".into(), kind: Kind::Computation, title: "compare arms".into(), url: String::new(),
            text: "RESULT diff = 0.12".into(), code: "print('RESULT diff =', d)".into(), partial: false }];
        let r = Report { confidence: 0.71, ..Default::default() };
        app.answers.push(((a, ev, r), ui::Stats { calls: 3, tokens: 1000, secs: 2.0 }));
        app.log = vec![Entry::User("Is the drug better?".into()), Entry::Answer(0)];
        app.tab = 1;
        app
    }

    fn screen(app: &mut App, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| app.draw(f)).unwrap();
        t.backend().buffer().content.iter().map(|c| c.symbol()).collect()
    }

    /// A real question through the spawned agent, the event sink and the finished-run path; prints the screen.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs API keys and network"]
    async fn live_run_end_to_end() {
        crate::config::load_env();
        let (tx, rx) = mpsc::channel();
        *ui::SINK.lock().unwrap() = Some(tx);
        let mut app = App::new(Some(Arc::new(Agent::new().unwrap())), vec![], rx);
        app.start("What is 17 * 23?".into());
        let t = Instant::now();
        while app.run.is_some() && t.elapsed() < Duration::from_secs(180) { app.pump().await; tokio::time::sleep(Duration::from_millis(100)).await; }
        *ui::SINK.lock().unwrap() = None;
        assert!(app.run.is_none(), "run did not finish");
        assert!(!app.steps.is_empty(), "no steps arrived through the sink");
        let mut t = Terminal::new(TestBackend::new(130, 40)).unwrap();
        for tab in [1, 4] {
            app.tab = tab;
            t.draw(|f| app.draw(f)).unwrap();
            let b = t.backend().buffer();
            for y in 0..40 { println!("{}", (0..130).map(|x| b[(x, y)].symbol()).collect::<String>().trim_end()); }
        }
        assert!(matches!(app.log.last(), Some(Entry::Answer(_))), "no answer in the transcript");
    }

    #[test]
    fn risky_claims_come_first() {
        let cs = [claim("a", Tag::Observed, &[]), claim("b", Tag::Speculation, &[]), claim("c", Tag::Inferred, &[]), claim("d", Tag::Observed, &[])];
        assert_eq!(claim_order(&cs, true), vec![1, 2, 0, 3]);
        assert_eq!(claim_order(&cs, false), vec![0, 1, 2, 3]);
    }

    #[test]
    fn wide_shows_answer_and_claims_side_by_side() {
        let mut app = app_with_answer();
        let s = screen(&mut app, 130, 34);
        assert!(s.contains("Yes, modestly"), "answer missing");
        assert!(s.contains("Claims") && s.contains("risky first"), "claims tab missing");
        assert!(s.contains("0.71"), "confidence missing");
        assert!(s.contains("no evidence cited"), "risky-first should select the uncited speculation first");
        app.sel = 1;
        assert!(screen(&mut app, 130, 34).contains("RESULT diff"), "evidence for the selected claim missing");
    }

    #[test]
    fn narrow_shows_one_pane() {
        let mut app = app_with_answer();
        let s = screen(&mut app, 80, 30);
        assert!(s.contains("Yes, modestly") && !s.contains("Reasoning"), "narrow should show the transcript only");
        app.show_panel = true;
        assert!(screen(&mut app, 80, 30).contains("Reasoning"));
    }

    #[test]
    fn home_draws_the_horse_when_there_is_room() {
        let (_tx, rx) = mpsc::channel();
        let mut app = App::new(None, vec![], rx);
        let big = screen(&mut app, 120, 48);
        assert!(big.contains("P A I D V A") && big.contains('⣿'));
        let small = screen(&mut app, 120, 24);
        assert!(small.contains("P A I D V A") && !small.contains('⣿'));
    }
}

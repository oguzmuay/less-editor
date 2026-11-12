//! Test yardımcısı: `|` imleç işaretli metin + tuş dizisi → işaretli sonuç.

#![allow(dead_code)]

use less_core::command::AppCommand;
use less_core::editor::{Editor, Session};
use less_core::selection::Selection;
use less_input::{EditMode, Engine, Output, parse_keys};

pub struct H {
    pub ed: Editor,
    pub s: Session,
    pub en: Engine,
    pub apps: Vec<AppCommand>,
    pub unhandled: usize,
}

impl H {
    pub fn new(input: &str, mode: EditMode) -> Self {
        let cursor = input.find('|').map(|b| input[..b].chars().count());
        let text = input.replacen('|', "", 1);
        let mut h = Self {
            ed: Editor::from_text(&text),
            s: Session::default(),
            en: Engine::new(mode),
            apps: Vec::new(),
            unhandled: 0,
        };
        for c in h.en.mode_commands() {
            h.ed.apply(c, &mut h.s);
        }
        h.ed.set_selection(Selection::point(cursor.unwrap_or(0)));
        h
    }

    pub fn vim(input: &str) -> Self {
        Self::new(input, EditMode::Vim)
    }

    pub fn modeless(input: &str) -> Self {
        Self::new(input, EditMode::Modeless)
    }

    pub fn keys(&mut self, keys: &str) -> &mut Self {
        for k in parse_keys(keys) {
            let out: Output = self.en.feed(k);
            if !out.handled {
                self.unhandled += 1;
            }
            for c in out.commands {
                if let Some(a) = self.ed.apply(c, &mut self.s) {
                    self.apps.push(a);
                }
            }
        }
        self
    }

    /// Metin, birincil imleç `|` ile.
    pub fn show(&self) -> String {
        let text = self.ed.text();
        let at = self.ed.cursor();
        let byte = text.char_indices().nth(at).map_or(text.len(), |(b, _)| b);
        format!("{}|{}", &text[..byte], &text[byte..])
    }

    /// Seçili metin(ler).
    pub fn selected(&self) -> Vec<String> {
        let text: Vec<char> = self.ed.text().chars().collect();
        self.ed.selection_spans().into_iter().map(|(a, b)| text[a..b].iter().collect()).collect()
    }

    pub fn last_message(&self) -> Option<String> {
        self.apps.iter().rev().find_map(|a| match a {
            AppCommand::Message(m) => Some(m.clone()),
            _ => None,
        })
    }
}

pub fn vim(input: &str, keys: &str) -> String {
    H::vim(input).keys(keys).show()
}

pub fn modeless(input: &str, keys: &str) -> String {
    H::modeless(input).keys(keys).show()
}

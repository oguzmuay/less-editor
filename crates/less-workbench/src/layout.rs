//! UI'dan bağımsız yerleşim modeli.
//!
//! Hedef düzen (CLAUDE.md "Genel yerleşim"): sağ sütun (AI üstte, terminal altta; Database ikisinin yerine) her zaman
//! tam boy; alt şerit sol paneller + kod alanının altına uzanır, sağ sütunun altına girmez (#29). Sol paneller Files
//! ya da Git (biri açılınca diğeri kapanır, #35). Şeritte tek araç (#19, #40). Tüm uzunluklar tek birimdedir
//! (UI px kullanır); hücre ölçüsü `Metrics` ile verilir.

/// Panel adı. `Code` her zaman vardır.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Panel {
    Files,
    Git,
    Code,
    Ai,
    Terminal,
    Database,
    /// Maven goal'ları; sağ sütunda Database'in yerinde (ikisinden biri açık).
    Maven,
    Run,
    GitHistory,
    Messages,
}

impl Panel {
    /// Kayıt dosyasındaki ad (LESS-nvim ile aynı: files, git, claude, terminal, db, runner, git_history).
    pub fn key(self) -> &'static str {
        match self {
            Panel::Files => "files",
            Panel::Git => "git",
            Panel::Code => "code",
            Panel::Ai => "claude",
            Panel::Terminal => "terminal",
            Panel::Database => "db",
            Panel::Maven => "maven",
            Panel::Run => "runner",
            Panel::GitHistory => "git_history",
            Panel::Messages => "messages",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        [
            Panel::Files,
            Panel::Git,
            Panel::Code,
            Panel::Ai,
            Panel::Terminal,
            Panel::Database,
            Panel::Maven,
            Panel::Run,
            Panel::GitHistory,
            Panel::Messages,
        ]
        .into_iter()
        .find(|p| p.key() == key)
    }

    fn strip_tool(self) -> Option<StripTool> {
        match self {
            Panel::Run => Some(StripTool::Run),
            Panel::GitHistory => Some(StripTool::GitHistory),
            Panel::Messages => Some(StripTool::Messages),
            _ => None,
        }
    }
}

/// Alt şeritteki araç.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StripTool {
    Run,
    GitHistory,
    Messages,
}

impl StripTool {
    pub fn panel(self) -> Panel {
        match self {
            StripTool::Run => Panel::Run,
            StripTool::GitHistory => Panel::GitHistory,
            StripTool::Messages => Panel::Messages,
        }
    }
}

/// Bir işlemin yan etkisi; UI süreç başlatma / odak vermede bunlara bakar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Opened(Panel),
    Closed(Panel),
    Focused(Panel),
    /// AI / terminal Database / Maven için gizlendi (süreç ölmez).
    Hidden(Panel),
    /// Database / Maven kapanınca AI / terminal geri geldi.
    Restored(Panel),
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// Hücre ölçüsü ve birim dönüşümü.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub cell_w: f32,
    pub cell_h: f32,
    /// 1 "pt"in kaç birim olduğu (egui noktasında 1.0).
    pub pt: f32,
}

impl Metrics {
    /// Font boyutundan: hücre genişliği `font_size × 0,6` (#96), yükseklik `font_size × 1,25`.
    pub fn from_font(font_size: f32) -> Self {
        Self { cell_w: font_size * 0.6, cell_h: font_size * 1.25, pt: 1.0 }
    }

    pub fn cols(&self, n: f32) -> f32 {
        n * self.cell_w
    }

    pub fn rows(&self, n: f32) -> f32 {
        n * self.cell_h
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::from_font(13.0)
    }
}

/// Boyut sınırları (#96): sütun/satır sayısı, ekran oranı ve mutlak `pt` üst sınırı.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub left_min_cols: f32,
    pub left_max_frac: f32,
    pub left_max_pt: f32,
    pub right_min_cols: f32,
    pub right_max_frac: f32,
    pub right_max_pt: f32,
    pub strip_min_rows: f32,
    pub strip_max_frac: f32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            left_min_cols: 20.0,
            left_max_frac: 0.3,
            left_max_pt: 610.0,
            right_min_cols: 30.0,
            right_max_frac: 0.5,
            right_max_pt: 800.0,
            strip_min_rows: 6.0,
            strip_max_frac: 0.5,
        }
    }
}

/// `(min, max)`; üst sınır asla alt sınırın altına düşmez.
fn bound(min: f32, frac: f32, total: f32, max_abs: f32) -> (f32, f32) {
    (min, (total * frac).min(max_abs).max(min))
}

impl Limits {
    pub fn left(&self, m: &Metrics, total_w: f32) -> (f32, f32) {
        bound(m.cols(self.left_min_cols), self.left_max_frac, total_w, self.left_max_pt * m.pt)
    }

    pub fn right(&self, m: &Metrics, total_w: f32) -> (f32, f32) {
        bound(m.cols(self.right_min_cols), self.right_max_frac, total_w, self.right_max_pt * m.pt)
    }

    pub fn strip(&self, m: &Metrics, total_h: f32) -> (f32, f32) {
        bound(m.rows(self.strip_min_rows), self.strip_max_frac, total_h, f32::MAX)
    }
}

/// Hatırlanan boyutlar (birim: px). `None` = varsayılan (`Layout::default_sizes`).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Sizes {
    pub files_w: Option<f32>,
    pub git_w: Option<f32>,
    pub right_w: Option<f32>,
    pub db_w: Option<f32>,
    /// AI'ın sağ sütundaki payı (0..1).
    pub ai_ratio: Option<f32>,
    pub strip_h: Option<f32>,
    pub run_tree_w: Option<f32>,
    pub vars_w: Option<f32>,
    pub branches_w: Option<f32>,
}

/// `compute` çıktısı.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Rects {
    pub left: Option<(Panel, Rect)>,
    pub code: Rect,
    pub ai: Option<Rect>,
    pub terminal: Option<Rect>,
    pub database: Option<Rect>,
    pub maven: Option<Rect>,
    /// Alt şeridin tamamı.
    pub strip: Option<(StripTool, Rect)>,
    /// Şerit içi parçalar: Run'da ağaç / log / Variables, Git History'de Branches / geçmiş.
    pub strip_parts: Vec<(StripPart, Rect)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StripPart {
    RunTree,
    RunLog,
    Variables,
    Branches,
    History,
    Messages,
}

/// Şerit içi sabit genişlikler için en az log / dar parça genişliği (sütun).
const LOG_MIN_COLS: f32 = 20.0;
const INNER_MIN_COLS: f32 = 20.0;

/// Yerleşim durumu.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub metrics: Metrics,
    pub limits: Limits,
    pub sizes: Sizes,
    left: Option<Panel>,
    ai: bool,
    terminal: bool,
    /// Sağ sütunu tek başına kaplayan panel (Database ya da Maven).
    side: Option<Panel>,
    ai_hidden_by_db: bool,
    terminal_hidden_by_db: bool,
    strip: Option<StripTool>,
    vars: bool,
    run_last_vars: bool,
    focus: Panel,
}

impl Default for Layout {
    fn default() -> Self {
        Self::new(Metrics::default())
    }
}

impl Layout {
    pub fn new(metrics: Metrics) -> Self {
        Self {
            metrics,
            limits: Limits::default(),
            sizes: Sizes::default(),
            left: None,
            ai: false,
            terminal: false,
            side: None,
            ai_hidden_by_db: false,
            terminal_hidden_by_db: false,
            strip: None,
            vars: false,
            run_last_vars: false,
            focus: Panel::Code,
        }
    }

    pub fn focus(&self) -> Panel {
        self.focus
    }

    /// Odağı elle (fare tıklaması) değiştirir; kapalı panele odak verilmez.
    pub fn set_focus(&mut self, panel: Panel) {
        if self.is_open(panel) {
            self.focus = panel;
        }
    }

    /// Panel şu an görünüyor mu (gizli AI / terminal görünmez sayılır).
    pub fn is_open(&self, panel: Panel) -> bool {
        match panel {
            Panel::Code => true,
            Panel::Files | Panel::Git => self.left == Some(panel),
            Panel::Ai => self.ai && self.side.is_none(),
            Panel::Terminal => self.terminal && self.side.is_none(),
            Panel::Database | Panel::Maven => self.side == Some(panel),
            Panel::Run | Panel::GitHistory | Panel::Messages => self.strip.map(StripTool::panel) == Some(panel),
        }
    }

    pub fn left(&self) -> Option<Panel> {
        self.left
    }

    pub fn strip(&self) -> Option<StripTool> {
        self.strip
    }

    /// Debug Variables şeritte görünüyor mu (#93).
    pub fn vars_visible(&self) -> bool {
        self.vars && self.strip == Some(StripTool::Run)
    }

    /// Açık panellerin listesi (kayıt için; açılış sırası: sol, AI, terminal, db, şerit).
    pub fn open_panels(&self) -> Vec<Panel> {
        [
            Panel::Files,
            Panel::Git,
            Panel::Ai,
            Panel::Terminal,
            Panel::Database,
            Panel::Maven,
            Panel::Run,
            Panel::GitHistory,
            Panel::Messages,
        ]
        .into_iter()
        .filter(|p| self.is_open(*p) || self.hidden_ai_terminal(*p))
        .collect()
    }

    fn hidden_ai_terminal(&self, p: Panel) -> bool {
        let db = self.side.is_some();
        (p == Panel::Ai && self.ai && db) || (p == Panel::Terminal && self.terminal && db)
    }

    /// Cmd+N tuşu davranışı (#5): kapalıysa aç, açıksa odakla, odaktaysa kapat ve koda dön.
    pub fn press(&mut self, panel: Panel) -> Vec<Effect> {
        if panel == Panel::Code {
            self.focus = Panel::Code;
            return vec![Effect::Focused(Panel::Code)];
        }
        if !self.is_open(panel) {
            return self.open(panel);
        }
        if self.focus == panel {
            return self.close(panel);
        }
        self.focus = panel;
        vec![Effect::Focused(panel)]
    }

    /// Saf aç/kapat (Ex komutları, #24): açıksa odakta olmasa da kapatır.
    pub fn toggle(&mut self, panel: Panel) -> Vec<Effect> {
        if panel == Panel::Code {
            return self.press(panel);
        }
        if self.is_open(panel) { self.close(panel) } else { self.open(panel) }
    }

    /// Paneli açar ve odak verir. Zaten açıksa yalnız odaklar.
    pub fn open(&mut self, panel: Panel) -> Vec<Effect> {
        let mut fx = Vec::new();
        match panel {
            Panel::Code => {}
            Panel::Files | Panel::Git => {
                if self.left == Some(panel) {
                    self.focus = panel;
                    return vec![Effect::Focused(panel)];
                }
                if let Some(old) = self.left.take() {
                    fx.push(Effect::Closed(old));
                }
                self.left = Some(panel);
            }
            Panel::Ai | Panel::Terminal => {
                if self.side.is_some() {
                    fx.extend(self.drop_db());
                }
                if panel == Panel::Ai {
                    self.ai = true;
                } else {
                    self.terminal = true;
                }
            }
            Panel::Database | Panel::Maven => {
                match self.side {
                    Some(p) if p == panel => {
                        self.focus = panel;
                        return vec![Effect::Focused(panel)];
                    }
                    Some(other) => fx.push(Effect::Closed(other)),
                    None => {
                        self.inherit_right_width();
                        self.ai_hidden_by_db = self.ai;
                        self.terminal_hidden_by_db = self.terminal;
                        if self.ai {
                            fx.push(Effect::Hidden(Panel::Ai));
                        }
                        if self.terminal {
                            fx.push(Effect::Hidden(Panel::Terminal));
                        }
                    }
                }
                self.side = Some(panel);
            }
            Panel::Run | Panel::GitHistory | Panel::Messages => {
                let tool = panel.strip_tool().expect("şerit paneli");
                if self.strip == Some(tool) {
                    self.focus = panel;
                    return vec![Effect::Focused(panel)];
                }
                if let Some(old) = self.strip.take() {
                    fx.extend(self.leave_strip(old));
                }
                self.strip = Some(tool);
                if tool == StripTool::Run {
                    self.vars = self.run_last_vars;
                }
            }
        }
        self.focus = panel;
        fx.push(Effect::Opened(panel));
        fx.push(Effect::Focused(panel));
        fx
    }

    /// Paneli kapatır; odak ona aitse koda döner. Database kapanırsa AI / terminal geri gelir (#45).
    pub fn close(&mut self, panel: Panel) -> Vec<Effect> {
        if panel == Panel::Code || !self.is_open(panel) {
            return Vec::new();
        }
        let mut fx = vec![Effect::Closed(panel)];
        match panel {
            Panel::Files | Panel::Git => self.left = None,
            Panel::Ai => self.ai = false,
            Panel::Terminal => self.terminal = false,
            Panel::Database | Panel::Maven => {
                self.side = None;
                if std::mem::take(&mut self.ai_hidden_by_db) {
                    fx.push(Effect::Restored(Panel::Ai));
                }
                if std::mem::take(&mut self.terminal_hidden_by_db) {
                    fx.push(Effect::Restored(Panel::Terminal));
                }
            }
            Panel::Run | Panel::GitHistory | Panel::Messages => {
                if let Some(t) = self.strip.take() {
                    fx.extend(self.leave_strip(t));
                }
            }
            Panel::Code => {}
        }
        self.focus = Panel::Code;
        fx
    }

    /// Şeritten çıkan araç: Run'ın Variables durumu hatırlanır (#94).
    fn leave_strip(&mut self, tool: StripTool) -> Vec<Effect> {
        if tool == StripTool::Run {
            self.run_last_vars = self.vars;
            self.vars = false;
        }
        vec![Effect::Closed(tool.panel())]
    }

    /// AI / terminal açılırken Database kapanır, geri yükleme iptal olur.
    fn drop_db(&mut self) -> Vec<Effect> {
        self.ai_hidden_by_db = false;
        self.terminal_hidden_by_db = false;
        self.side.take().map(Effect::Closed).into_iter().collect()
    }

    fn inherit_right_width(&mut self) {
        if (self.ai || self.terminal) && self.sizes.right_w.is_some() {
            self.sizes.db_w = self.sizes.right_w;
        }
    }

    /// Debug oturumu breakpoint'te durdu (#93): Run açık değilse açar (odak değişmez), Variables'ı ekler.
    pub fn show_variables(&mut self) -> Vec<Effect> {
        let focus = self.focus;
        let mut fx = Vec::new();
        if self.strip != Some(StripTool::Run) {
            fx.extend(self.open(Panel::Run));
            self.focus = focus;
        }
        self.vars = true;
        fx
    }

    /// Variables'ı gizler (Run açık kalır).
    pub fn hide_variables(&mut self) {
        self.vars = false;
        self.run_last_vars = false;
    }

    /// `Space d u`: Variables'ı aç / kapat.
    pub fn toggle_variables(&mut self) -> Vec<Effect> {
        if self.vars_visible() {
            self.hide_variables();
            Vec::new()
        } else {
            self.show_variables()
        }
    }

    /// `:q` koruması (#41): sekmedeki son kod penceresi kapatılamaz.
    pub fn may_close_code_window(code_windows: usize) -> bool {
        code_windows > 1
    }

    pub fn set_left_width(&mut self, w: f32) {
        match self.left {
            Some(Panel::Git) => self.sizes.git_w = Some(w),
            _ => self.sizes.files_w = Some(w),
        }
    }

    pub fn set_right_width(&mut self, w: f32) {
        if self.side.is_some() {
            self.sizes.db_w = Some(w);
        } else {
            self.sizes.right_w = Some(w);
        }
    }

    pub fn set_ai_ratio(&mut self, r: f32) {
        self.sizes.ai_ratio = Some(r.clamp(0.1, 0.9));
    }

    pub fn set_strip_height(&mut self, h: f32) {
        self.sizes.strip_h = Some(h);
    }

    /// Varsayılan boyutlar (#42) verilen toplam boyut için.
    pub fn default_sizes(&self, total_w: f32, total_h: f32) -> Sizes {
        let m = &self.metrics;
        Sizes {
            files_w: Some(m.cols(34.0)),
            git_w: Some(m.cols(40.0)),
            right_w: Some((total_w / m.cell_w * 0.38).floor() * m.cell_w),
            db_w: Some(m.cols(40.0)),
            ai_ratio: Some(0.5),
            strip_h: Some(m.rows(10.0).max((total_h / m.cell_h * 0.3).floor() * m.cell_h)),
            run_tree_w: Some(m.cols(56.0)),
            vars_w: None,
            branches_w: Some(m.cols(34.0)),
        }
    }

    /// `:UIReset` (#42): hatırlanan boyutlar silinir, varsayılanlar kurulur.
    pub fn reset(&mut self, total_w: f32, total_h: f32) {
        self.sizes = self.default_sizes(total_w, total_h);
    }

    /// Hatırlanan boyutları sınırlara çeker (#96), `compute` zaten uygular; fareyle sürüklerken çağrılır.
    pub fn clamp_sizes(&mut self, total_w: f32, total_h: f32) {
        let m = self.metrics;
        let (lmin, lmax) = self.limits.left(&m, total_w);
        let (rmin, rmax) = self.limits.right(&m, total_w);
        let (smin, smax) = self.limits.strip(&m, total_h);
        let c = |v: &mut Option<f32>, lo: f32, hi: f32| *v = v.map(|x| x.clamp(lo, hi));
        c(&mut self.sizes.files_w, lmin, lmax);
        c(&mut self.sizes.git_w, lmin, lmax);
        c(&mut self.sizes.right_w, rmin, rmax);
        c(&mut self.sizes.db_w, rmin, rmax);
        c(&mut self.sizes.strip_h, smin, smax);
    }

    fn size_or(&self, v: Option<f32>, total_w: f32, total_h: f32, pick: fn(&Sizes) -> Option<f32>) -> f32 {
        v.or_else(|| pick(&self.default_sizes(total_w, total_h))).unwrap_or(0.0)
    }

    /// Bölgeleri hesaplar. Boyutlar sınırlara çekilir; sağ sütun tam boy, şerit sağ sütunun solunda biter (#29).
    pub fn compute(&self, total_w: f32, total_h: f32) -> Rects {
        let m = self.metrics;
        let mut out = Rects::default();
        let db = self.side.is_some();
        let right_visible = db || self.ai || self.terminal;
        let (rmin, rmax) = self.limits.right(&m, total_w);
        let right_w = if right_visible {
            let v = if db { self.sizes.db_w.or(self.sizes.right_w) } else { self.sizes.right_w };
            let pick: fn(&Sizes) -> Option<f32> = if db { |s| s.db_w } else { |s| s.right_w };
            self.size_or(v, total_w, total_h, pick).clamp(rmin, rmax)
        } else {
            0.0
        };
        let main_w = (total_w - right_w).max(0.0);

        let (smin, smax) = self.limits.strip(&m, total_h);
        let strip_h = match self.strip {
            Some(_) => self.size_or(self.sizes.strip_h, total_w, total_h, |s| s.strip_h).clamp(smin, smax.min(total_h)),
            None => 0.0,
        };
        let top_h = (total_h - strip_h).max(0.0);

        let (lmin, lmax) = self.limits.left(&m, total_w);
        let left_w = match self.left {
            Some(p) => {
                let w = if p == Panel::Git {
                    self.size_or(self.sizes.git_w, total_w, total_h, |s| s.git_w)
                } else {
                    self.size_or(self.sizes.files_w, total_w, total_h, |s| s.files_w)
                };
                let max = lmax.min(main_w * 0.8);
                w.clamp(lmin.min(max), max)
            }
            None => 0.0,
        };
        if let Some(p) = self.left {
            out.left = Some((p, Rect::new(0.0, 0.0, left_w, top_h)));
        }
        out.code = Rect::new(left_w, 0.0, (main_w - left_w).max(0.0), top_h);

        if right_visible {
            let x = main_w;
            if let Some(side) = self.side {
                let r = Some(Rect::new(x, 0.0, right_w, total_h));
                if side == Panel::Maven { out.maven = r } else { out.database = r }
            } else if self.ai && self.terminal {
                let ratio = self.sizes.ai_ratio.unwrap_or(0.5).clamp(0.1, 0.9);
                let ai_h = (total_h * ratio).round();
                out.ai = Some(Rect::new(x, 0.0, right_w, ai_h));
                out.terminal = Some(Rect::new(x, ai_h, right_w, total_h - ai_h));
            } else if self.ai {
                out.ai = Some(Rect::new(x, 0.0, right_w, total_h));
            } else {
                out.terminal = Some(Rect::new(x, 0.0, right_w, total_h));
            }
        }

        if let Some(tool) = self.strip {
            let area = Rect::new(0.0, top_h, main_w, strip_h);
            out.strip = Some((tool, area));
            out.strip_parts = self.strip_parts(tool, area);
        }
        out
    }

    /// Şerit içi bölme. Run: ağaç (sabit) | log (esner, #102) | Variables (sabit); log en az 20 sütun, dar kalırsa
    /// önce Variables sonra ağaç kısılır. Git History: Branches (sabit) | geçmiş (esner).
    fn strip_parts(&self, tool: StripTool, area: Rect) -> Vec<(StripPart, Rect)> {
        let m = self.metrics;
        let cut = |x: f32, w: f32| Rect::new(area.x + x, area.y, w, area.h);
        match tool {
            StripTool::Messages => vec![(StripPart::Messages, area)],
            StripTool::GitHistory => {
                let b = self.sizes.branches_w.unwrap_or(m.cols(34.0)).min(area.w * 0.5);
                vec![(StripPart::Branches, cut(0.0, b)), (StripPart::History, cut(b, area.w - b))]
            }
            StripTool::Run => {
                let mut tree = self.sizes.run_tree_w.unwrap_or(m.cols(56.0));
                let mut vars = if self.vars_visible() {
                    self.sizes.vars_w.unwrap_or((area.w * 0.3).max(m.cols(40.0)))
                } else {
                    0.0
                };
                let log_min = m.cols(LOG_MIN_COLS);
                let inner_min = m.cols(INNER_MIN_COLS);
                let mut deficit = tree + vars + log_min - area.w;
                if deficit > 0.0 && vars > 0.0 {
                    let take = deficit.min((vars - inner_min).max(0.0));
                    vars -= take;
                    deficit -= take;
                }
                if deficit > 0.0 {
                    tree -= deficit.min((tree - inner_min).max(0.0));
                }
                let log = (area.w - tree - vars).max(0.0);
                let mut parts = vec![(StripPart::RunTree, cut(0.0, tree)), (StripPart::RunLog, cut(tree, log))];
                if vars > 0.0 {
                    parts.push((StripPart::Variables, cut(tree + log, vars)));
                }
                parts
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f32 = 1600.0;
    const H: f32 = 900.0;

    fn layout() -> Layout {
        let mut l = Layout::default();
        l.reset(W, H);
        l
    }

    #[test]
    fn press_opens_focuses_then_closes_5() {
        let mut l = layout();
        l.press(Panel::Files);
        assert_eq!(l.focus(), Panel::Files);
        l.press(Panel::Code);
        assert_eq!(l.press(Panel::Files), vec![Effect::Focused(Panel::Files)]);
        let fx = l.press(Panel::Files);
        assert_eq!(fx, vec![Effect::Closed(Panel::Files)]);
        assert_eq!(l.focus(), Panel::Code);
    }

    #[test]
    fn toggle_closes_even_unfocused_24() {
        let mut l = layout();
        l.open(Panel::Ai);
        l.press(Panel::Code);
        l.toggle(Panel::Ai);
        assert!(!l.is_open(Panel::Ai));
    }

    #[test]
    fn files_and_git_exclusive_35() {
        let mut l = layout();
        l.open(Panel::Files);
        let fx = l.open(Panel::Git);
        assert!(fx.contains(&Effect::Closed(Panel::Files)));
        assert!(!l.is_open(Panel::Files) && l.is_open(Panel::Git));
    }

    #[test]
    fn strip_one_tool_at_a_time_19_40() {
        let mut l = layout();
        l.open(Panel::Run);
        l.open(Panel::GitHistory);
        assert!(!l.is_open(Panel::Run) && l.is_open(Panel::GitHistory));
        l.open(Panel::Messages);
        assert!(!l.is_open(Panel::GitHistory) && l.is_open(Panel::Messages));
    }

    #[test]
    fn right_column_ai_over_terminal_full_height_6_20() {
        let mut l = layout();
        l.open(Panel::Terminal);
        l.open(Panel::Ai);
        l.open(Panel::Run);
        let r = l.compute(W, H);
        let (ai, term) = (r.ai.unwrap(), r.terminal.unwrap());
        assert_eq!(ai.y, 0.0);
        assert_eq!(term.bottom(), H);
        assert_eq!(ai.bottom(), term.y);
        assert_eq!(ai.x, term.x);
    }

    #[test]
    fn strip_stops_at_right_column_29() {
        let mut l = layout();
        l.open(Panel::Files);
        l.open(Panel::Ai);
        l.open(Panel::Run);
        let r = l.compute(W, H);
        let strip = r.strip.unwrap().1;
        assert_eq!(strip.x, 0.0);
        assert_eq!(strip.right(), r.ai.unwrap().x);
        assert_eq!(strip.bottom(), H);
        let left = r.left.unwrap().1;
        assert_eq!(left.bottom(), strip.y);
        assert_eq!(r.code.bottom(), strip.y);
    }

    #[test]
    fn database_hides_and_restores_ai_terminal_45() {
        let mut l = layout();
        l.open(Panel::Ai);
        l.open(Panel::Terminal);
        let fx = l.open(Panel::Database);
        assert!(fx.contains(&Effect::Hidden(Panel::Ai)));
        assert!(!l.is_open(Panel::Ai));
        let fx = l.close(Panel::Database);
        assert!(fx.contains(&Effect::Restored(Panel::Ai)) && fx.contains(&Effect::Restored(Panel::Terminal)));
        assert!(l.is_open(Panel::Ai) && l.is_open(Panel::Terminal));
    }

    #[test]
    fn opening_ai_drops_database_without_restore_45() {
        let mut l = layout();
        l.open(Panel::Ai);
        l.open(Panel::Database);
        l.open(Panel::Terminal);
        assert!(!l.is_open(Panel::Database));
        assert!(l.is_open(Panel::Ai));
        assert!(l.is_open(Panel::Terminal));
    }

    #[test]
    fn maven_takes_database_slot_and_keeps_hidden_ai() {
        let mut l = layout();
        l.open(Panel::Ai);
        l.open(Panel::Database);
        let fx = l.open(Panel::Maven);
        assert!(fx.contains(&Effect::Closed(Panel::Database)));
        assert!(l.is_open(Panel::Maven) && !l.is_open(Panel::Database) && !l.is_open(Panel::Ai));
        let r = l.compute(W, H);
        assert!(r.maven.is_some() && r.database.is_none());
        let fx = l.close(Panel::Maven);
        assert!(fx.contains(&Effect::Restored(Panel::Ai)) && l.is_open(Panel::Ai));
    }

    #[test]
    fn database_inherits_right_width_45() {
        let mut l = layout();
        l.open(Panel::Ai);
        l.set_right_width(500.0);
        l.open(Panel::Database);
        assert_eq!(l.compute(W, H).database.unwrap().w, 500.0);
    }

    #[test]
    fn debug_variables_inside_run_strip_93() {
        let mut l = layout();
        l.open(Panel::Files);
        l.press(Panel::Code);
        l.show_variables();
        assert!(l.is_open(Panel::Run) && l.vars_visible());
        assert_eq!(l.focus(), Panel::Code);
        let r = l.compute(W, H);
        assert!(r.strip_parts.iter().any(|(p, _)| *p == StripPart::Variables));
    }

    #[test]
    fn run_remembers_variables_94() {
        let mut l = layout();
        l.show_variables();
        l.close(Panel::Run);
        l.open(Panel::GitHistory);
        l.close(Panel::GitHistory);
        l.open(Panel::Run);
        assert!(l.vars_visible());
    }

    #[test]
    fn limits_clamp_left_30_percent_and_610pt_96() {
        let mut l = layout();
        l.open(Panel::Files);
        l.set_left_width(5000.0);
        let w = l.compute(3000.0, H).left.unwrap().1.w;
        assert_eq!(w, 610.0);
        let w = l.compute(1000.0, H).left.unwrap().1.w;
        assert_eq!(w, 300.0);
        l.set_left_width(1.0);
        assert_eq!(l.compute(W, H).left.unwrap().1.w, l.metrics.cols(20.0));
    }

    #[test]
    fn limits_clamp_right_50_percent_and_800pt_96() {
        let mut l = layout();
        l.open(Panel::Ai);
        l.set_right_width(5000.0);
        assert_eq!(l.compute(1200.0, H).ai.unwrap().w, 600.0);
        assert_eq!(l.compute(4000.0, H).ai.unwrap().w, 800.0);
    }

    #[test]
    fn limits_clamp_strip_height_96() {
        let mut l = layout();
        l.open(Panel::Run);
        l.set_strip_height(10_000.0);
        let (_, s) = l.compute(W, H).strip.unwrap();
        assert_eq!(s.h, H * 0.5);
        l.set_strip_height(0.0);
        let (_, s) = l.compute(W, H).strip.unwrap();
        assert_eq!(s.h, l.metrics.rows(6.0));
    }

    #[test]
    fn clamp_sizes_rewrites_remembered_96() {
        let mut l = layout();
        l.set_right_width(9000.0);
        l.clamp_sizes(W, H);
        assert_eq!(l.sizes.right_w, Some(800.0));
    }

    #[test]
    fn reset_defaults_42() {
        let mut l = Layout::default();
        l.reset(1600.0, 900.0);
        let m = l.metrics;
        let cols = 1600.0 / m.cell_w;
        assert_eq!(l.sizes.right_w, Some((cols * 0.38).floor() * m.cell_w));
        assert_eq!(l.sizes.files_w, Some(m.cols(34.0)));
        assert_eq!(l.sizes.db_w, Some(m.cols(40.0)));
        assert_eq!(l.sizes.git_w, Some(m.cols(40.0)));
        assert_eq!(l.sizes.run_tree_w, Some(m.cols(56.0)));
        let rows = 900.0 / m.cell_h;
        assert_eq!(l.sizes.strip_h, Some((rows * 0.3).floor() * m.cell_h));
    }

    #[test]
    fn strip_log_flexes_and_tree_keeps_width_102() {
        let mut l = layout();
        l.open(Panel::Run);
        l.show_variables();
        l.open(Panel::Ai);
        let wide = l.compute(2400.0, H);
        let narrow = l.compute(1000.0, H);
        let tree = |r: &Rects| r.strip_parts.iter().find(|(p, _)| *p == StripPart::RunTree).unwrap().1.w;
        let log = |r: &Rects| r.strip_parts.iter().find(|(p, _)| *p == StripPart::RunLog).unwrap().1.w;
        assert_eq!(tree(&wide), l.metrics.cols(56.0));
        assert!(log(&wide) > log(&narrow));
        assert!(log(&narrow) >= l.metrics.cols(20.0) - 0.01);
    }

    #[test]
    fn strip_parts_cover_area_without_gaps_102() {
        let mut l = layout();
        l.show_variables();
        let r = l.compute(W, H);
        let (_, area) = r.strip.unwrap();
        let mut x = area.x;
        for (_, p) in &r.strip_parts {
            assert!((p.x - x).abs() < 0.01);
            x = p.right();
        }
        assert!((x - area.right()).abs() < 0.01);
    }

    #[test]
    fn code_window_guard_41() {
        assert!(!Layout::may_close_code_window(1));
        assert!(Layout::may_close_code_window(2));
    }

    #[test]
    fn closing_focused_panel_returns_focus_to_code_5() {
        let mut l = layout();
        l.open(Panel::Run);
        l.close(Panel::Run);
        assert_eq!(l.focus(), Panel::Code);
    }

    #[test]
    fn open_panels_lists_hidden_ai_for_persistence_45() {
        let mut l = layout();
        l.open(Panel::Ai);
        l.open(Panel::Database);
        let p = l.open_panels();
        assert!(p.contains(&Panel::Ai) && p.contains(&Panel::Database));
    }
}

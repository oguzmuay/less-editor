//! Run paneli: Spring / Node keşfi, süreç ve log yönetimi, DAP köprüsü ve debug değişken ağacı.
//!
//! Giriş noktaları: [`Runner`] (ağaç, süreçler, `poll()` olayları), [`shutdown::Shutdown`] (çıkışta düzgün kapanış),
//! [`dap::DebugManager`] (attach, oturum olayları), [`variables::VariablesModel`], [`breakpoints::BreakpointStore`].

pub mod breakpoints;
pub mod command;
pub mod config;
pub mod dap;
pub mod model;
pub mod output;
pub mod paths;
pub mod ports;
pub mod process;
pub mod providers;
pub mod runner;
pub mod shutdown;
pub mod variables;

pub use command::{CommandSpec, Mode};
pub use config::{ConfigStore, RunConfig};
pub use model::{BuildTool, Child, Group, GroupKind, ItemKind, ItemSpec, PackageManager};
pub use runner::{AttachTarget, ItemState, Level, RunEvent, Runner, Status};

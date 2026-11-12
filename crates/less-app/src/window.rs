//! Yeni LESS-rs penceresi: mevcut yürütülebilir dosyanın yeni bir örneğini başlatır (#53 #61 #66 #75).
//! `std::process::Command` kullanılır; native menü ya da macOS uygulama kontrolü yoktur.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Child, Command, Stdio};

use crate::app::Target;

/// Yeni örneğin argümanları. Testler bu saf çıktıyı doğrular, süreç başlatmaz.
pub fn args(target: Option<&Target>) -> Vec<OsString> {
    match target {
        Some(Target::Folder(path)) => vec![path.as_os_str().to_owned()],
        Some(Target::Workspace(ws)) => vec![OsString::from("--workspace"), OsString::from(&ws.name)],
        None => Vec::new(),
    }
}

/// Yeni süreç / pencere açar. GUI doğrulaması bu fonksiyonu testlerde çağırmaz.
pub fn spawn(executable: &Path, target: Option<&Target>) -> std::io::Result<Child> {
    Command::new(executable).args(args(target)).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
}

#[cfg(test)]
mod tests {
    use super::*;
    use less_workspace::workspaces::Workspace;
    use std::path::PathBuf;

    #[test]
    fn new_window_arguments_preserve_folder_and_workspace_61_66() {
        assert!(args(None).is_empty());
        assert_eq!(args(Some(&Target::Folder(PathBuf::from("/work/api")))), [OsString::from("/work/api")]);
        let ws = Workspace { name: "team".into(), roots: Vec::new(), opened: 0 };
        assert_eq!(args(Some(&Target::Workspace(ws))), [OsString::from("--workspace"), OsString::from("team")]);
    }
}

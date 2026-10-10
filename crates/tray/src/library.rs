//! The PlayStation games on the card: one entry per game, however many
//! files it spans.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The files a game can be started from, best first. A playlist names
/// every disc of a game and a cue names a disc's tracks, so each is
/// preferred over the images it points at.
const KINDS: [&str; 8] = ["m3u", "cue", "chd", "pbp", "bin", "img", "iso", "exe"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    /// The file the core is given.
    pub path: PathBuf,
    /// The file name without its extension, for label art and the title.
    pub stem: String,
    /// The name shown on the shelf, without region and version tags.
    pub title: String,
}

fn kind(path: &Path) -> Option<usize> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    KINDS.iter().position(|k| *k == ext)
}

/// The files a playlist or cue sheet refers to, resolved against its
/// folder. A playlist lists one path a line; a cue names each track's file
/// on a FILE line, quoted or not.
fn referenced(path: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    text.lines()
        .map(str::trim)
        .filter_map(|line| match ext.as_deref() {
            Some("m3u") => (!line.is_empty() && !line.starts_with('#')).then(|| line.to_string()),
            Some("cue") => cue_file(line),
            _ => None,
        })
        .map(|name| dir.join(name))
        .collect()
}

fn cue_file(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("FILE ")
        .or_else(|| line.strip_prefix("file "))?;
    let rest = rest.trim();
    if let Some(quoted) = rest.strip_prefix('"') {
        return Some(quoted.split('"').next()?.to_string());
    }
    // Unquoted: the name runs up to the file type at the end of the line.
    let (name, _kind) = rest.rsplit_once(' ')?;
    Some(name.trim().to_string())
}

/// Every game in `dir`, sorted by title. Files a playlist or cue in the
/// folder refers to are part of that game, not games of their own.
pub fn scan(dir: &Path) -> Vec<Game> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && !slot_store::is_hidden(p) && kind(p).is_some())
        .collect();
    let lower = |p: &Path| p.to_string_lossy().to_lowercase();
    let taken: HashSet<String> = files
        .iter()
        .filter(|p| matches!(kind(p), Some(0 | 1)))
        .flat_map(|p| referenced(p))
        .map(|p| lower(&p))
        .collect();
    let mut games: Vec<Game> = files
        .into_iter()
        .filter(|p| !taken.contains(&lower(p)))
        .map(|path| {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let title = slot_ui::clean_label(&stem);
            Game { path, stem, title }
        })
        .collect();
    games.sort_by(|a, b| {
        (a.title.to_lowercase(), kind(&a.path)).cmp(&(b.title.to_lowercase(), kind(&b.path)))
    });
    games
}

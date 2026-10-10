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
    /// The file the game starts from: a playlist, a disc, or for discs
    /// grouped by name, the first of them.
    pub path: PathBuf,
    /// The game's name as a file stem, without any disc number. Its memory
    /// card, label art and remembered disc are named after it.
    pub stem: String,
    /// The name shown on the shelf, without region and version tags.
    pub title: String,
    /// Every disc, in order, each as the core will name it.
    pub discs: Vec<PathBuf>,
    /// Discs grouped by their names, with no playlist on the card: tray
    /// writes one for the core when the game starts.
    pub grouped: bool,
}

impl Game {
    fn single(path: PathBuf) -> Game {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let discs = match kind(&path) {
            Some(0) => playlist_discs(&path),
            _ => vec![path.clone()],
        };
        Game {
            title: slot_ui::clean_label(&stem),
            stem,
            discs,
            path,
            grouped: false,
        }
    }
}

/// The disc number in a name such as "Game (USA) (Disc 2)" or
/// "(Disc 2 of 3)", and the name without it.
pub fn disc_number(stem: &str) -> Option<(u32, String)> {
    let lower = stem.to_ascii_lowercase();
    let at = lower.find("(disc ")?;
    let rest = &lower[at + 6..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let n: u32 = digits.parse().ok()?;
    let mut tail = &rest[digits.len()..];
    if let Some(of) = tail.strip_prefix(" of ") {
        tail = of.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    let close = tail.strip_prefix(')')?;
    let end = stem.len() - close.len();
    let bare = format!("{} {}", stem[..at].trim_end(), stem[end..].trim_start());
    Some((n, bare.trim().to_string()))
}

/// The discs a playlist lists, as PCSX ReARMed reads them: each line not
/// starting with '#' is a path, taken as it is if absolute and otherwise
/// from the playlist's folder.
fn playlist_discs(path: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| dir.join(l))
        .collect()
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
    let mut games: Vec<Game> = Vec::new();
    let mut numbered: std::collections::BTreeMap<String, Vec<(u32, PathBuf)>> =
        std::collections::BTreeMap::new();
    for path in files.into_iter().filter(|p| !taken.contains(&lower(p))) {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        match (kind(&path), disc_number(&stem)) {
            (Some(k), Some((n, bare))) if k != 0 => {
                numbered.entry(bare).or_default().push((n, path))
            }
            _ => games.push(Game::single(path)),
        }
    }
    // Discs numbered in their names are one game, even with no playlist.
    for (bare, mut discs) in numbered {
        discs.sort();
        discs.dedup_by_key(|d| d.0);
        if discs.len() == 1 {
            games.push(Game::single(discs.remove(0).1));
            continue;
        }
        let discs: Vec<PathBuf> = discs.into_iter().map(|d| d.1).collect();
        games.push(Game {
            path: discs[0].clone(),
            title: slot_ui::clean_label(&bare),
            stem: bare,
            discs,
            grouped: true,
        });
    }
    games.sort_by(|a, b| {
        (a.title.to_lowercase(), kind(&a.path)).cmp(&(b.title.to_lowercase(), kind(&b.path)))
    });
    games
}

/// Where tray writes the playlist for discs grouped by name.
pub const PLAYLISTS: &str = "Saves/PS/.playlists";

/// The file to hand the core for `game`: its own file, or for discs grouped
/// by name, a playlist of them written under `root`.
pub fn content(root: &Path, game: &Game) -> std::io::Result<PathBuf> {
    if !game.grouped {
        return Ok(game.path.clone());
    }
    let dir = root.join(PLAYLISTS);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.m3u", game.stem));
    let mut text = String::new();
    for disc in &game.discs {
        text.push_str(&disc.to_string_lossy());
        text.push('\n');
    }
    std::fs::write(&path, text)?;
    Ok(path)
}

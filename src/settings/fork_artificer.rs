//! FORK(artificer): the fork's default theme pair (`artificer-dark` /
//! `artificer-light`) is two shipped theme files, not presets, so a theme
//! name has to resolve even before `themes/` is written (a fresh app home,
//! unit tests). [`theme_text`] reads the file and falls back to the copy
//! embedded in [`super::THEMES`].

/// The text of theme `name`: the file at `path`, or, when that can't be read,
/// the embedded copy of a shipped theme. A file that exists always wins, so
/// a person's edits to a shipped theme still show.
pub(super) fn theme_text(path: &std::path::Path, name: &str) -> std::io::Result<String> {
    std::fs::read_to_string(path).or_else(|e| {
        super::THEMES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, t)| t.to_string())
            .ok_or(e)
    })
}

#[cfg(test)]
mod tests {
    use super::super::{DEFAULT_DARK, DEFAULT_LIGHT, THEMES, is_light, parse_theme_file};
    use super::theme_text;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("herdr-fork-artificer-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn default_themes_resolve_without_their_files() {
        let dir = temp_dir("missing");
        for name in [DEFAULT_DARK, DEFAULT_LIGHT] {
            let text = theme_text(&dir.join(format!("{name}.json")), name)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(
                text,
                THEMES.iter().find(|(n, _)| *n == name).unwrap().1,
                "{name}"
            );
            let (palette, _) = parse_theme_file(name, &text).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(
                is_light(palette.background),
                name == DEFAULT_LIGHT,
                "{name}"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_theme_file_on_disk_wins_over_the_embedded_copy() {
        let dir = temp_dir("present");
        let path = dir.join(format!("{DEFAULT_DARK}.json"));
        std::fs::write(&path, "edited").unwrap();
        assert_eq!(theme_text(&path, DEFAULT_DARK).unwrap(), "edited");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_unknown_missing_theme_is_an_error() {
        let dir = temp_dir("unknown");
        assert!(theme_text(&dir.join("no-such-theme.json"), "no-such-theme").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

use crate::diag::{Diagnostic, Severity};
use crate::FailureClass;

const ORIGIN: &str = "compilers.java.fixer";

fn fail(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(
        code,
        Severity::Error,
        FailureClass::UserError,
        ORIGIN,
        message,
    )
}

pub const REPAIR_TITLES: &[&str] = &[
    "Declare the missing resource in the project's own values folder.",
    "Remove the attribute that names a resource this project does not hold.",
    "Set the manifest's package from the package its sources declare.",
    "Write the activity's name out in full.",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repair {
    pub code: String,
    pub path: String,
    pub title: String,
    pub before: String,
    pub after: String,
}

impl Repair {
    pub fn spoken_title(&self) -> String {
        crate::speech::sentence(&self.title)
    }

    pub fn write_json(&self, w: &mut crate::json::Writer) {
        w.begin_object(None);
        w.field_str("code", &self.code);
        w.field_str("path", &self.path);
        w.field_str("title", &self.spoken_title());
        w.field_str("english", &self.title);
        w.field_str("before", &self.before);
        w.field_str("after", &self.after);
        w.end_object();
    }
}

fn context_value(refusal: &Diagnostic, label: &str) -> Option<String> {
    let head = format!("{label}: ");
    refusal
        .context
        .iter()
        .find_map(|line| line.strip_prefix(head.as_str()))
        .map(|held| held.trim().to_string())
}

fn manifest_at(root: &str) -> std::path::PathBuf {
    std::path::Path::new(root).join("AndroidManifest.xml")
}

fn attribute_text(source: &str, name: &str) -> Option<String> {
    let mut at = 0usize;
    let held = source.as_bytes();
    while let Some(found) = source[at..].find(name) {
        let start = at + found;
        let before = start.checked_sub(1).map(|one| held[one]);
        if !matches!(before, Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')) {
            at = start + name.len();
            continue;
        }
        let rest = &source[start + name.len()..];
        let trimmed = rest.trim_start();
        if !trimmed.starts_with('=') {
            at = start + name.len();
            continue;
        }
        let after_equals = &trimmed[1..];
        let quoted = after_equals.trim_start();
        let mark = quoted.chars().next()?;
        if mark != '"' && mark != '\'' {
            return None;
        }
        let closing = quoted[1..].find(mark)? + 1;
        let consumed =
            (rest.len() - trimmed.len()) + 1 + (after_equals.len() - quoted.len()) + closing + 1;
        return Some(source[start..start + name.len() + consumed].to_string());
    }
    None
}

fn declared_package(root: &str) -> Option<String> {
    let sources = std::path::Path::new(root).join("Java");
    let mut found: Option<String> = None;
    let mut waiting = vec![sources];
    while let Some(here) = waiting.pop() {
        let Ok(entries) = std::fs::read_dir(&here) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                waiting.push(path);
                continue;
            }
            if path.extension().is_none_or(|held| held != "java") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in text.lines() {
                let line = line.trim();
                let Some(rest) = line.strip_prefix("package ") else {
                    continue;
                };
                let named = rest.trim().trim_end_matches(';').trim();
                if named.is_empty() {
                    continue;
                }
                if found.is_none() {
                    found = Some(named.to_string());
                }
                break;
            }
        }
    }
    found
}

fn values_folder(root: &str) -> std::path::PathBuf {
    std::path::Path::new(root).join("Res").join("values")
}

fn declaration_for(kind: &str, name: &str) -> Option<String> {
    let held = match kind {
        "style" => format!(
            "    <style name=\"{name}\" parent=\"android:Theme.Material.Light.NoActionBar\" />"
        ),
        "string" => format!("    <string name=\"{name}\">{name}</string>"),
        "color" => format!("    <color name=\"{name}\">#FF000000</color>"),
        "bool" => format!("    <bool name=\"{name}\">false</bool>"),
        "integer" => format!("    <integer name=\"{name}\">0</integer>"),
        "dimen" => format!("    <dimen name=\"{name}\">0dp</dimen>"),
        _ => return None,
    };
    Some(held)
}

pub fn repairs_for(root: &str, refusal: &Diagnostic) -> Vec<Repair> {
    let mut out = Vec::new();
    match refusal.code.as_str() {
        "EB042" | "EB045" => {
            let (Some(attribute), Some(value)) = (
                context_value(refusal, "Attribute"),
                context_value(refusal, "Value"),
            ) else {
                return out;
            };
            let Ok(source) = std::fs::read_to_string(manifest_at(root)) else {
                return out;
            };
            if let Some(written) = attribute_text(&source, &attribute) {
                out.push(Repair {
                    code: refusal.code.clone(),
                    path: "AndroidManifest.xml".to_string(),
                    title: "Remove the attribute that names a resource this project does not hold."
                        .to_string(),
                    before: written,
                    after: String::new(),
                });
            }
            let named = value.trim_start_matches('@');
            if let Some((kind, name)) = named.split_once('/') {
                let kind = kind.rsplit(':').next().unwrap_or(kind);
                if let Some(line) = declaration_for(kind, name) {
                    let file = format!("Res/values/{kind}s.xml");
                    let path = values_folder(root).join(format!("{kind}s.xml"));
                    let (before, after) = match std::fs::read_to_string(&path) {
                        Ok(held) if held.contains("</resources>") => (
                            "</resources>".to_string(),
                            format!("{line}\n</resources>"),
                        ),
                        _ => (
                            String::new(),
                            format!(
                                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n{line}\n</resources>\n"
                            ),
                        ),
                    };
                    out.push(Repair {
                        code: refusal.code.clone(),
                        path: file,
                        title: "Declare the missing resource in the project's own values folder."
                            .to_string(),
                        before,
                        after,
                    });
                }
            }
        }
        "EB013" | "EB014" => {
            let Ok(source) = std::fs::read_to_string(manifest_at(root)) else {
                return out;
            };
            let Some(package) = declared_package(root) else {
                return out;
            };
            if attribute_text(&source, "package").is_none() {
                if let Some(found) = source.find("<manifest") {
                    let head = &source[found..];
                    let Some(stop) = head.find('>') else {
                        return out;
                    };
                    let opening = &head[..stop];
                    out.push(Repair {
                        code: refusal.code.clone(),
                        path: "AndroidManifest.xml".to_string(),
                        title: "Set the manifest's package from the package its sources declare."
                            .to_string(),
                        before: opening.to_string(),
                        after: format!("{opening}\n    package=\"{package}\""),
                    });
                }
            }
            if let Some(written) = attribute_text(&source, "android:name") {
                if let Some(opened) = written.find('"') {
                    let value = &written[opened + 1..written.len() - 1];
                    if let Some(rest) = value.strip_prefix('.') {
                        out.push(Repair {
                            code: refusal.code.clone(),
                            path: "AndroidManifest.xml".to_string(),
                            title: "Write the activity's name out in full.".to_string(),
                            before: written.clone(),
                            after: format!("android:name=\"{package}.{rest}\""),
                        });
                    }
                }
            }
        }
        _ => {}
    }
    out
}

pub fn apply(root: &str, repair: &Repair) -> Result<(), Diagnostic> {
    let named = std::path::Path::new(&repair.path);
    let wanders = named.components().any(|one| {
        matches!(
            one,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    });
    if wanders || repair.path.is_empty() {
        return Err(fail(
            "EF001",
            "A repair names a file that is not in this project.",
        )
        .with_context(format!("Path: {}", repair.path)));
    }
    let path = std::path::Path::new(root).join(named);
    if repair.before.is_empty() {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(|why| {
                fail("EF003", "A repair could not be written.")
                    .with_context(format!("Path: {}", repair.path))
                    .with_context(format!("Cause: {why}"))
            })?;
        }
        std::fs::write(&path, &repair.after).map_err(|why| {
            fail("EF003", "A repair could not be written.")
                .with_context(format!("Path: {}", repair.path))
                .with_context(format!("Cause: {why}"))
        })?;
        return Ok(());
    }
    let source = std::fs::read_to_string(&path).map_err(|why| {
        fail("EF003", "A repair could not be written.")
            .with_context(format!("Path: {}", repair.path))
            .with_context(format!("Cause: {why}"))
    })?;
    let Some(at) = source.find(&repair.before) else {
        return Err(fail(
            "EF002",
            "A repair no longer matches the file it was made for.",
        )
        .with_context(format!("Path: {}", repair.path)));
    };
    let mut written = String::with_capacity(source.len() + repair.after.len());
    written.push_str(&source[..at]);
    written.push_str(&repair.after);
    written.push_str(&source[at + repair.before.len()..]);
    std::fs::write(&path, written).map_err(|why| {
        fail("EF003", "A repair could not be written.")
            .with_context(format!("Path: {}", repair.path))
            .with_context(format!("Cause: {why}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(label: &str) -> std::path::PathBuf {
        let mut at = std::env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        at.push(format!("{label}-{stamp}"));
        std::fs::create_dir_all(&at).unwrap();
        at
    }

    fn refusal(code: &str, attribute: &str, value: &str) -> Diagnostic {
        fail(
            code,
            "The manifest names a resource this project does not hold.",
        )
        .with_context(format!("Attribute: {attribute}"))
        .with_context(format!("Value: {value}"))
    }

    const MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <application
        android:label="@string/app_name"
        android:theme="@style/AppTheme">
        <activity
            android:name=".MainActivity"
            android:exported="true" />
    </application>
</manifest>
"#;

    #[test]
    fn a_missing_style_is_offered_both_ways_and_either_one_holds() {
        let root = temp("omni-fixer-style");
        let folder = root.to_str().unwrap().to_string();
        std::fs::write(root.join("AndroidManifest.xml"), MANIFEST).unwrap();

        let offered = repairs_for(
            &folder,
            &refusal("EB042", "android:theme", "@style/AppTheme"),
        );
        assert_eq!(offered.len(), 2, "{offered:#?}");

        let dropped = offered
            .iter()
            .find(|one| one.before.contains("android:theme"))
            .expect("one repair drops the attribute");
        apply(&folder, dropped).expect("it applies");
        let after = std::fs::read_to_string(root.join("AndroidManifest.xml")).unwrap();
        assert!(!after.contains("android:theme"), "{after}");
        assert!(
            after.contains("android:label"),
            "it took only that one: {after}"
        );

        std::fs::write(root.join("AndroidManifest.xml"), MANIFEST).unwrap();
        let declared = offered
            .iter()
            .find(|one| one.path.ends_with("styles.xml"))
            .expect("one repair declares the style");
        apply(&folder, declared).expect("it applies");
        let written = std::fs::read_to_string(root.join("Res/values/styles.xml")).unwrap();
        assert!(written.contains("name=\"AppTheme\""), "{written}");
        assert!(written.contains("</resources>"), "{written}");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_manifest_with_no_package_takes_the_one_its_sources_declare() {
        let root = temp("omni-fixer-package");
        let folder = root.to_str().unwrap().to_string();
        std::fs::write(root.join("AndroidManifest.xml"), MANIFEST).unwrap();
        let sources = root.join("Java");
        std::fs::create_dir_all(&sources).unwrap();
        std::fs::write(
            sources.join("MainActivity.java"),
            "package com.example.cube3d;\n\npublic class MainActivity {}\n",
        )
        .unwrap();

        let offered = repairs_for(&folder, &fail("EB013", "The manifest could not be read."));
        assert_eq!(offered.len(), 2, "{offered:#?}");
        for one in &offered {
            apply(&folder, one).expect("it applies");
        }
        let after = std::fs::read_to_string(root.join("AndroidManifest.xml")).unwrap();
        assert!(after.contains("package=\"com.example.cube3d\""), "{after}");
        assert!(
            after.contains("android:name=\"com.example.cube3d.MainActivity\""),
            "{after}"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_repair_that_no_longer_matches_is_refused_rather_than_guessed_at() {
        let root = temp("omni-fixer-stale");
        let folder = root.to_str().unwrap().to_string();
        std::fs::write(root.join("AndroidManifest.xml"), MANIFEST).unwrap();
        let offered = repairs_for(
            &folder,
            &refusal("EB042", "android:theme", "@style/AppTheme"),
        );
        let dropped = offered
            .iter()
            .find(|one| one.before.contains("android:theme"))
            .unwrap()
            .clone();
        apply(&folder, &dropped).expect("the first one holds");
        let error = apply(&folder, &dropped).expect_err("the second does not");
        assert_eq!(error.code, "EF002");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_repair_cannot_reach_outside_the_project() {
        let root = temp("omni-fixer-escape");
        let folder = root.to_str().unwrap().to_string();
        let error = apply(
            &folder,
            &Repair {
                code: "EB042".to_string(),
                path: "../outside.xml".to_string(),
                title: "Write the activity's name out in full.".to_string(),
                before: String::new(),
                after: "no".to_string(),
            },
        )
        .expect_err("a path that leaves the project is refused");
        assert_eq!(error.code, "EF001");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn every_repair_title_is_one_the_languages_carry() {
        let _turn = crate::progress::one_at_a_time();
        crate::speech::choose("tr");
        for title in REPAIR_TITLES {
            let spoken = crate::speech::sentence(title);
            assert_ne!(spoken, *title, "{title} is not translated");
        }
        crate::speech::choose("en");
    }
}

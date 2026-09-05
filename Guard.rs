use crate::diag::{Diagnostic, Severity, Sink};
use crate::json::Writer;
use crate::xml::Element;
use crate::FailureClass;

pub const MINIMUM_SDK: i64 = crate::compiler::OLDEST_API as i64;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    Passed,
    Refused,
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub rule: &'static str,
    pub code: &'static str,
    pub what: String,
    pub why: String,
    pub remedy: String,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub rules_applied: usize,
}

impl Report {
    pub fn verdict(&self) -> Verdict {
        if self.findings.is_empty() {
            Verdict::Passed
        } else {
            Verdict::Refused
        }
    }

    pub fn write_json(&self, w: &mut Writer, key: &str) {
        w.begin_object(Some(key));
        w.field_str(
            "verdict",
            match self.verdict() {
                Verdict::Passed => "PASSED",
                Verdict::Refused => "REFUSED",
            },
        );
        w.field_u64("rulesApplied", self.rules_applied as u64);
        w.begin_array(Some("findings"));
        for finding in &self.findings {
            w.begin_object(None);
            w.field_str("rule", finding.rule);
            w.field_str("code", finding.code);
            w.field_str("what", &finding.what);
            w.field_str("why", &finding.why);
            w.field_str("remedy", &finding.remedy);
            w.end_object();
        }
        w.end_array();
        w.field_str(
            "note",
            "These rules are the ones a manifest can be checked against. Passing them says the package does not carry these specific weaknesses; it is not a statement that the application is secure.",
        );
        w.end_object();
    }
}

fn attribute<'a>(element: &'a Element, name: &str) -> Option<&'a str> {
    element
        .attributes
        .iter()
        .find(|attribute| attribute.name == name)
        .map(|attribute| attribute.value.as_str())
}

fn walk<'a>(element: &'a Element, out: &mut Vec<&'a Element>) {
    out.push(element);
    for child in &element.children {
        walk(child, out);
    }
}

pub const RULES: &[(&str, &str, &str)] = &[
    (
        "EG001",
        "no-debuggable",
        "Nothing on the device may attach a debugger to it.",
    ),
    (
        "EG002",
        "no-cleartext",
        "Its traffic is not carried in the open.",
    ),
    (
        "EG003",
        "no-backup",
        "Its data is not copied off the device by the backup service.",
    ),
    (
        "EG004",
        "guard-exported",
        "What other applications may start is said, and what they must hold to start it.",
    ),
    (
        "EG005",
        "provider-uri-grants",
        "A content provider does not hand out access to everything it holds.",
    ),
    (
        "EG006",
        "minimum-platform",
        "It runs on platforms that still receive fixes.",
    ),
    (
        "EG007",
        "high-risk-permission",
        "It does not ask for a permission that reads everything a person does.",
    ),
    (
        "EG008",
        "no-shared-user",
        "It does not share a user with another application.",
    ),
    (
        "EG009",
        "no-legacy-storage",
        "It does not opt out of the storage rules the platform put there.",
    ),
    (
        "EG010",
        "no-package-census",
        "It does not ask to see every application installed.",
    ),
    (
        "EG011",
        "no-task-hijacking",
        "Another application cannot place its own screen inside this one's task.",
    ),
    (
        "EG012",
        "no-ignored-restriction",
        "It does not carry a restriction the platform it targets ignores.",
    ),
];

pub const ADAPTIVE_FROM_API: i64 = 37;

const IGNORED_AT_37: &[(&str, Option<&str>)] = &[
    ("android:screenOrientation", None),
    ("android:resizeableActivity", Some("false")),
    ("android:minAspectRatio", None),
    ("android:maxAspectRatio", None),
];

pub fn fired(report: &Report, code: &str) -> bool {
    report.findings.iter().any(|held| held.code == code)
}

pub fn inspect_manifest(root: &Element) -> Report {
    let mut report = Report::default();
    let mut elements = Vec::new();
    walk(root, &mut elements);

    let application = elements
        .iter()
        .find(|element| element.name == "application");

    report.rules_applied += 1;
    if let Some(application) = application {
        if attribute(application, "android:debuggable") == Some("true") {
            report.findings.push(Finding {
                rule: "no-debuggable",
                code: "EG001",
                what: "The application is marked debuggable.".to_string(),
                why: "A debuggable application lets any process on the device attach a debugger, read its memory and run code as it.".to_string(),
                remedy: "Remove android:debuggable, or set it to false.".to_string(),
            });
        }
    }

    report.rules_applied += 1;
    if let Some(application) = application {
        if attribute(application, "android:usesCleartextTraffic") == Some("true") {
            report.findings.push(Finding {
                rule: "no-cleartext",
                code: "EG002",
                what: "The application permits cleartext HTTP.".to_string(),
                why: "Traffic without TLS can be read and rewritten by anything on the path.".to_string(),
                remedy: "Set android:usesCleartextTraffic to false, or configure a network security policy that allows it for named hosts only.".to_string(),
            });
        }
    }

    report.rules_applied += 1;
    if let Some(application) = application {
        match attribute(application, "android:allowBackup") {
            Some("false") => {}
            _ => report.findings.push(Finding {
                rule: "no-backup",
                code: "EG003",
                what: "The application allows its data to be backed up.".to_string(),
                why: "With backup enabled, application data can be copied off the device over adb without unlocking it.".to_string(),
                remedy: "Set android:allowBackup to false, or declare rules that exclude anything sensitive.".to_string(),
            }),
        }
    }

    report.rules_applied += 1;
    for element in &elements {
        if !matches!(
            element.name.as_str(),
            "activity" | "service" | "receiver" | "provider"
        ) {
            continue;
        }
        let exported = attribute(element, "android:exported") == Some("true");
        let has_filter = element
            .children
            .iter()
            .any(|child| child.name == "intent-filter");
        let guarded = attribute(element, "android:permission").is_some();
        let launcher = element.children.iter().any(|filter| {
            filter.name == "intent-filter"
                && filter.children.iter().any(|entry| {
                    entry.name == "category"
                        && attribute(entry, "android:name")
                            == Some("android.intent.category.LAUNCHER")
                })
        });
        if exported && has_filter && !guarded && !launcher {
            let name = attribute(element, "android:name").unwrap_or("(unnamed)");
            report.findings.push(Finding {
                rule: "guard-exported",
                code: "EG004",
                what: format!("{} {name} is exported with no permission.", element.name),
                why: "Any application on the device can reach an exported component that no permission guards.".to_string(),
                remedy: "Set android:exported to false, or require a permission with android:permission.".to_string(),
            });
        }
    }

    report.rules_applied += 1;
    for element in &elements {
        if element.name != "provider" {
            continue;
        }
        if attribute(element, "android:grantUriPermissions") == Some("true")
            && attribute(element, "android:exported") == Some("true")
        {
            let name = attribute(element, "android:name").unwrap_or("(unnamed)");
            report.findings.push(Finding {
                rule: "provider-uri-grants",
                code: "EG005",
                what: format!("provider {name} is exported and grants URI permissions."),
                why: "An exported provider that hands out URI grants can be made to give a caller access to files it should not reach.".to_string(),
                remedy: "Set android:exported to false; a content provider is reachable through grants without being exported.".to_string(),
            });
        }
    }

    report.rules_applied += 1;
    let declared = elements
        .iter()
        .find(|element| element.name == "uses-sdk")
        .and_then(|element| attribute(element, "android:minSdkVersion"))
        .and_then(|text| text.parse::<i64>().ok());
    match declared {
        Some(value) if value >= MINIMUM_SDK => {}
        _ => report.findings.push(Finding {
            rule: "minimum-platform",
            code: "EG006",
            what: format!("The package does not require API {MINIMUM_SDK} or later."),
            why: "Below API 30 the platform does not require a v2 signature, permits cleartext by default, and lacks the scoped storage and hardware-backed key protections the rules above assume.".to_string(),
            remedy: "Declare a minimum platform that still receives fixes.".to_string(),
        }),
    }

    report.rules_applied += 1;
    for element in &elements {
        if element.name != "uses-permission" {
            continue;
        }
        let name = attribute(element, "android:name").unwrap_or("");
        const REFUSED: &[(&str, &str)] = &[
            (
                "android.permission.REQUEST_INSTALL_PACKAGES",
                "installing other applications",
            ),
            (
                "android.permission.MANAGE_EXTERNAL_STORAGE",
                "reading all of shared storage",
            ),
            (
                "android.permission.SYSTEM_ALERT_WINDOW",
                "drawing over other applications",
            ),
            (
                "android.permission.BIND_ACCESSIBILITY_SERVICE",
                "observing and acting on every screen",
            ),
        ];
        if let Some((_, what)) = REFUSED.iter().find(|(permission, _)| *permission == name) {
            report.findings.push(Finding {
                rule: "high-risk-permission",
                code: "EG007",
                what: format!("The package requests {name}, which grants {what}."),
                why: "This permission reaches far beyond what a build tool's output needs, \
                       and is the usual shape of an abusive application."
                    .to_string(),
                remedy: "Remove the permission. If the application genuinely needs it, it is not something this build engine will produce without the request being made deliberately and reviewed.".to_string(),
            });
        }
    }

    report.rules_applied += 1;
    if let Some(shared) = root.attribute("android:sharedUserId") {
        report.findings.push(Finding {
            rule: "no-shared-user",
            code: "EG008",
            what: format!("The package shares a user identity, {shared}."),
            why: "Every application signed with the same key and naming the same identity runs as one user: they read each other's files, databases and keys with no check of any kind. One weak application among them is a way into all of them, and Android has deprecated the whole idea.".to_string(),
            remedy: "Remove android:sharedUserId. Where two applications must exchange something, let one expose it deliberately through a provider or a service and guard it with a signature permission.".to_string(),
        });
    }

    report.rules_applied += 1;
    if let Some(application) = application {
        if attribute(application, "android:requestLegacyExternalStorage") == Some("true") {
            report.findings.push(Finding {
                rule: "no-legacy-storage",
                code: "EG009",
                what: "The application asks to opt out of scoped storage.".to_string(),
                why: "Legacy external storage lets the application read and write everything on shared storage, including what every other application has left there. It is what scoped storage exists to end.".to_string(),
                remedy: "Remove android:requestLegacyExternalStorage. Use the media store for what is shared, and the application's own folders for what is not.".to_string(),
            });
        }
    }

    report.rules_applied += 1;
    for element in &elements {
        if element.name != "uses-permission" {
            continue;
        }
        if attribute(element, "android:name") == Some("android.permission.QUERY_ALL_PACKAGES") {
            report.findings.push(Finding {
                rule: "no-package-census",
                code: "EG010",
                what: "The package asks to see every application installed.".to_string(),
                why: "The list of what somebody has installed says a great deal about them, and reading all of it is how tracking libraries build a profile. Android narrowed this deliberately in API 30.".to_string(),
                remedy: "Remove the permission and declare a <queries> element naming the few applications or intents this one genuinely needs to find.".to_string(),
            });
        }
    }

    report.rules_applied += 1;
    for element in &elements {
        if element.name != "activity" && element.name != "activity-alias" {
            continue;
        }

        let exported = match attribute(element, "android:exported") {
            Some("true") => true,
            Some("false") => false,
            _ => element.children_named("intent-filter").next().is_some(),
        };
        let mode = attribute(element, "android:launchMode").unwrap_or("standard");
        if exported && (mode == "singleTask" || mode == "singleInstance") {
            let named = attribute(element, "android:name").unwrap_or("an activity");
            report.findings.push(Finding {
                rule: "no-task-hijacking",
                code: "EG011",
                what: format!("{named} is reachable from outside and launches as {mode}."),
                why: "An activity another application can start, in a task of its own, can have that task taken over: a malicious application declares the same affinity, gets placed in front, and what the person types next goes to it instead. This is task hijacking, and it looks exactly like the real screen.".to_string(),
                remedy: "Use the standard launch mode for anything exported, or set android:taskAffinity to an empty string so no other application can join its task.".to_string(),
            });
        }
    }

    report.rules_applied += 1;
    let targets = elements
        .iter()
        .find(|element| element.name == "uses-sdk")
        .and_then(|element| attribute(element, "android:targetSdkVersion"))
        .and_then(|held| held.parse::<i64>().ok())
        .unwrap_or(0);
    let a_game = application
        .map(|held| attribute(held, "android:appCategory") == Some("game"))
        .unwrap_or(false);
    if targets >= ADAPTIVE_FROM_API && !a_game {
        for element in &elements {
            if element.name != "application" && element.name != "activity" {
                continue;
            }
            for (name, refused) in IGNORED_AT_37 {
                let Some(value) = attribute(element, name) else {
                    continue;
                };
                if refused.is_some_and(|held| held != value) {
                    continue;
                }
                let named = attribute(element, "android:name").unwrap_or(&element.name);
                report.findings.push(Finding {
                    rule: "no-ignored-restriction",
                    code: "EG012",
                    what: format!("{named} sets {name}=\"{value}\"."),
                    why: "An application targeting API 37 or later has this ignored on \
                          any display wider than 600dp: the window fills the screen whatever \
                          the attribute says. A restriction that looks like it is in force \
                          and is not is worse than one that was never written."
                        .to_string(),
                    remedy: "Remove the attribute, and lay the screen out for whatever size \
                             it is given. Games are exempt, and say so with \
                             android:appCategory=\"game\"."
                        .to_string(),
                });
            }
        }
    }

    report
}

pub fn said(code: &str) -> &'static str {
    RULES
        .iter()
        .find(|(held, _, _)| *held == code)
        .map(|(_, _, sentence)| *sentence)
        .unwrap_or("The project does not meet the security policy.")
}

pub fn emit(report: &Report, sink: &mut Sink) {
    for finding in &report.findings {
        sink.emit(
            Diagnostic::new(
                finding.code,
                Severity::Fatal,
                FailureClass::SecurityFailure,
                "core.guard",
                said(finding.code),
            )
            .with_context(format!("Found: {}", finding.what))
            .with_context(finding.why.clone())
            .with_suggestion(finding.remedy.clone()),
        );
    }
}

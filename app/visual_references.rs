//! Bounded, automatic reference lookup for procedural answer visuals.
//! Search results are untrusted evidence, never scene code or renderer URLs.
use std::io::Cursor;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine;
use image::{DynamicImage, ImageDecoder, ImageReader, codecs::jpeg::JpegEncoder};
use serde_json::Value;

#[derive(Clone)]
pub struct Reference {
    pub title: String,
    pub source: String,
    pub page: String,
    pub jpeg_base64: String,
}

pub fn query_for_scene(_label: &str, request: &str) -> String {
    // The user request is authoritative. An agent-generated scene label may describe
    // a completely different object and must never steer reference retrieval.
    let subject = request;
    let words: Vec<&str> = subject
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != ' ')
        .flat_map(str::split_whitespace)
        // Qualifiers describe the requested view, not the identity to search.
        // "pickup truck with an open bed" should search for "pickup truck".
        .take_while(|word| {
            !matches!(
                word.to_ascii_lowercase().as_str(),
                "with"
                    | "showing"
                    | "including"
                    | "featuring"
                    | "having"
                    | "where"
                    | "that"
                    | "and"
            )
        })
        .filter(|word| {
            !matches!(
                word.to_ascii_lowercase().as_str(),
                "schematic"
                    | "detailed"
                    | "highly"
                    | "realistic"
                    | "accurate"
                    | "high"
                    | "quality"
                    | "procedural"
                    | "live"
                    | "animated"
                    | "full"
                    | "visualize"
                    | "visualise"
                    | "visulaize"
                    | "exploded"
                    | "draw"
                    | "show"
                    | "make"
                    | "create"
                    | "illustrate"
                    | "please"
                    | "can"
                    | "you"
                    | "me"
                    | "a"
                    | "an"
                    | "the"
                    | "of"
                    | "for"
                    | "how"
                    | "model"
                    | "visualization"
                    | "concept"
                    | "parts"
                    | "manual"
                    | "mechanism"
                    | "interactive"
                    | "construction"
                    | "view"
                    | "diagram"
                    | "not"
                    | "to"
                    | "scale"
                    | "3d"
            )
        })
        .take(5)
        .collect();
    words.join(" ").chars().take(72).collect()
}

/// Only use an exact subject gate for short, unambiguous visual requests.
/// Longer requests still go through the model's semantic review.
pub fn single_subject(request: &str) -> Option<String> {
    let subject = query_for_scene("", request);
    let mut words = subject.split_whitespace();
    let first = words.next()?;
    (first.chars().count() >= 3 && words.next().is_none()).then(|| first.to_lowercase())
}

/// Explicit source and destination forms need distinct reference searches.
fn endpoint_queries(request: &str) -> Option<[String; 2]> {
    let lower = request.to_ascii_lowercase();
    let markers = [
        "'s transformation from ",
        "’s transformation from ",
        " transformation from ",
        " transforming from ",
        " transforms from ",
        " transform from ",
    ];
    let (start, marker) = markers
        .into_iter()
        .filter_map(|marker| lower.find(marker).map(|start| (start, marker)))
        .min_by_key(|(start, _)| *start)?;
    let forms = start + marker.len();
    let split = forms + lower[forms..].find(" to ")?;
    let subject = query_for_scene("", &request[..start]);
    let source = query_for_scene("", &request[forms..split]);
    let destination = query_for_scene("", &request[split + 4..]);
    if subject.is_empty()
        || source.is_empty()
        || destination.is_empty()
        || source.eq_ignore_ascii_case(&destination)
    {
        return None;
    }
    Some([
        format!("{subject} {source}"),
        format!("{subject} {destination}"),
    ])
}

pub fn search_request(request: &str, preflight: bool) -> Vec<Reference> {
    if let Some([source, destination]) = endpoint_queries(request) {
        let budget = if preflight {
            Duration::from_secs(2)
        } else {
            Duration::from_secs(5)
        };
        return thread::scope(|scope| {
            let source = scope.spawn(|| search_with_budget(&source, budget, 2));
            let destination = search_with_budget(&destination, budget, 2);
            select_endpoint_references(source.join().unwrap_or_default(), destination)
        });
    }
    let subject = query_for_scene("", request);
    let (budget, limit) = if preflight {
        (Duration::from_millis(1800), 1)
    } else {
        (Duration::from_secs(5), 2)
    };
    search_with_budget(&subject, budget, limit)
}

fn reference_variant(title: &str) -> Option<String> {
    let title = title.to_ascii_lowercase();
    title
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .find_map(|word| {
            ["gen", "mk", "g", "v"].into_iter().find_map(|prefix| {
                let suffix = word.strip_prefix(prefix)?;
                (!suffix.is_empty()
                    && suffix.len() <= 2
                    && suffix.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| word.to_owned())
            })
        })
}

fn select_endpoint_references(
    mut source: Vec<Reference>,
    mut destination: Vec<Reference>,
) -> Vec<Reference> {
    let mut compatible = None;
    for (source_index, first) in source.iter().enumerate() {
        for (destination_index, second) in destination.iter().enumerate() {
            if first.jpeg_base64 == second.jpeg_base64 {
                continue;
            }
            match (
                reference_variant(&first.title),
                reference_variant(&second.title),
            ) {
                (Some(a), Some(b)) if a == b => {
                    return vec![
                        source.remove(source_index),
                        destination.remove(destination_index),
                    ];
                }
                (Some(_), Some(_)) => {}
                _ => {
                    compatible.get_or_insert((source_index, destination_index));
                }
            }
        }
    }
    if let Some((a, b)) = compatible {
        return vec![source.remove(a), destination.remove(b)];
    }
    source.into_iter().chain(destination).take(1).collect()
}

fn usable_reference_title(title: &str, subject: &str) -> bool {
    let title = title.to_ascii_lowercase().replace(['-', '_'], " ");
    let subject = subject.to_ascii_lowercase().replace(['-', '_'], " ");
    let unfinished = |text: &str| {
        text.contains("work in progress")
            || text
                .split(|ch: char| !ch.is_ascii_alphanumeric())
                .any(|word| word == "wip")
    };
    !unfinished(&title) || unfinished(&subject)
}

type ReferenceCache = std::collections::VecDeque<(String, Instant, Vec<Reference>)>;
static REFERENCE_CACHE: std::sync::OnceLock<std::sync::Mutex<ReferenceCache>> =
    std::sync::OnceLock::new();

fn search_with_budget(subject: &str, budget: Duration, limit: usize) -> Vec<Reference> {
    let cache = REFERENCE_CACHE.get_or_init(Default::default);
    let key = subject.trim().to_lowercase();
    if let Ok(entries) = cache.lock()
        && let Some((_, _, references)) = entries
            .iter()
            .find(|(name, time, _)| name == &key && time.elapsed() < Duration::from_secs(300))
    {
        return references.iter().take(limit).cloned().collect();
    }

    if subject.trim().is_empty() {
        return Vec::new();
    }
    let (tx, rx) = mpsc::channel();
    for source in [0, 1, 2] {
        let tx = tx.clone();
        let subject = subject.to_owned();
        thread::spawn(move || {
            let reference = match source {
                0 => search_wikipedia(&subject),
                1 => search_openverse(&subject).into_iter().next(),
                _ => search_commons(&subject, 1).into_iter().next(),
            };
            if let Some(reference) = reference {
                let _ = tx.send(reference);
            }
        });
    }
    drop(tx);
    let deadline = Instant::now() + budget;
    let mut found = Vec::new();
    while found.len() < limit {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok(reference) => {
                // A fast search result is useful only if its metadata actually
                // identifies the requested subject. Models still review images.
                let metadata = format!("{} {}", reference.title, reference.page).to_lowercase();
                let relevant = subject
                    .split_whitespace()
                    .all(|word| metadata.contains(&word.to_lowercase()));
                if relevant && usable_reference_title(&reference.title, subject) {
                    found.push(reference);
                }
            }
            Err(_) => break,
        }
    }
    if !found.is_empty()
        && let Ok(mut entries) = cache.lock()
    {
        entries.retain(|(name, time, _)| name != &key && time.elapsed() < Duration::from_secs(300));
        while entries.len() >= 4 {
            entries.pop_front();
        }
        entries.push_back((key, Instant::now(), found.clone()));
    }
    found
}

fn search_wikipedia(subject: &str) -> Option<Reference> {
    let raw = fetch(
        "https://en.wikipedia.org/w/api.php",
        &[
            ("action", "query"),
            ("format", "json"),
            ("titles", subject),
            ("prop", "pageimages|info"),
            ("pithumbsize", "512"),
            ("inprop", "url"),
            ("redirects", "1"),
        ],
        150_000,
    )?;
    let value: Value = serde_json::from_slice(&raw).ok()?;
    let pages = value["query"]["pages"].as_object()?;
    for page in pages.values() {
        let (Some(title), Some(thumb), Some(url)) = (
            page["title"].as_str(),
            page["thumbnail"]["source"].as_str(),
            page["fullurl"].as_str(),
        ) else {
            continue;
        };
        if !plausible_title(title, subject)
            || !thumb.starts_with("https://upload.wikimedia.org/")
            || !url.starts_with("https://en.wikipedia.org/wiki/")
        {
            continue;
        }
        let bytes = fetch(thumb, &[], 600_000)?;
        let jpeg_base64 = normalized_jpeg(&bytes)?;
        return Some(Reference {
            title: title.chars().take(100).collect(),
            source: "Wikipedia".into(),
            page: url.chars().take(300).collect(),
            jpeg_base64,
        });
    }
    None
}

fn fetch(url: &str, fields: &[(&str, &str)], limit: usize) -> Option<Vec<u8>> {
    let mut command = Command::new("curl.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.args([
        "--silent",
        "--show-error",
        "--fail",
        "--proto",
        "=https",
        "--user-agent",
        "Pluto-Desktop/0.1.6 (visual-reference-review)",
        "--connect-timeout",
        "2",
        "--max-time",
        "4",
        "--max-filesize",
        &limit.to_string(),
        "--get",
    ]);
    for (key, value) in fields {
        command
            .arg("--data-urlencode")
            .arg(format!("{key}={value}"));
    }
    let output = command.arg(url).output().ok()?;
    if output.status.success() && output.stdout.len() <= limit {
        Some(output.stdout)
    } else {
        None
    }
}

fn normalized_jpeg(bytes: &[u8]) -> Option<String> {
    if bytes.len() > 600_000 {
        return None;
    }
    let decoder = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || u64::from(width) * u64::from(height) > 12_000_000
    {
        return None;
    }
    let image = DynamicImage::from_decoder(decoder).ok()?.resize(
        512,
        512,
        image::imageops::FilterType::Triangle,
    );
    let mut output = Vec::new();
    JpegEncoder::new_with_quality(&mut output, 70)
        .encode_image(&image)
        .ok()?;
    if output.len() > 120_000 {
        return None;
    }
    Some(base64::engine::general_purpose::STANDARD.encode(output))
}

fn plausible_title(title: &str, subject: &str) -> bool {
    let title = title.to_lowercase();
    let terms: Vec<_> = subject
        .split_whitespace()
        .filter(|word| word.len() >= 3)
        .collect();
    !terms.is_empty()
        && terms
            .iter()
            .all(|word| title.contains(&word.to_lowercase()))
}

fn search_openverse(subject: &str) -> Vec<Reference> {
    let Some(raw) = fetch(
        "https://api.openverse.org/v1/images/",
        &[("q", subject), ("page_size", "8"), ("mature", "false")],
        300_000,
    ) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_slice::<Value>(&raw) else {
        return Vec::new();
    };
    let Some(items) = value["results"].as_array() else {
        return Vec::new();
    };
    let mut refs = Vec::new();
    for item in items {
        if !refs.is_empty() {
            break;
        }
        let (Some(id), Some(title), Some(page)) = (
            item["id"].as_str(),
            item["title"].as_str(),
            item["foreign_landing_url"].as_str(),
        ) else {
            continue;
        };
        if !is_uuid(id)
            || !plausible_title(title, subject)
            || !usable_reference_title(title, subject)
            || !page.starts_with("https://")
        {
            continue;
        }
        let url = format!("https://api.openverse.org/v1/images/{id}/thumb/");
        let Some(bytes) = fetch(&url, &[], 600_000) else {
            continue;
        };
        let Some(jpeg_base64) = normalized_jpeg(&bytes) else {
            continue;
        };
        refs.push(Reference {
            title: title.chars().take(100).collect(),
            source: "Openverse".into(),
            page: page.chars().take(300).collect(),
            jpeg_base64,
        });
    }
    refs
}

fn search_commons(subject: &str, count: usize) -> Vec<Reference> {
    let Some(raw) = fetch(
        "https://commons.wikimedia.org/w/api.php",
        &[
            ("action", "query"),
            ("format", "json"),
            ("generator", "search"),
            ("gsrsearch", subject),
            ("gsrnamespace", "6"),
            ("gsrlimit", "8"),
            ("prop", "imageinfo"),
            ("iiprop", "url"),
            ("iiurlwidth", "512"),
        ],
        300_000,
    ) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_slice::<Value>(&raw) else {
        return Vec::new();
    };
    let Some(pages) = value["query"]["pages"].as_object() else {
        return Vec::new();
    };
    let mut refs = Vec::new();
    for item in pages.values() {
        if refs.len() == count {
            break;
        }
        let (Some(title), Some(info)) = (item["title"].as_str(), item["imageinfo"].get(0)) else {
            continue;
        };
        if !plausible_title(title, subject) || !usable_reference_title(title, subject) {
            continue;
        }
        let (Some(thumb), Some(page)) =
            (info["thumburl"].as_str(), info["descriptionurl"].as_str())
        else {
            continue;
        };
        if !thumb.starts_with("https://upload.wikimedia.org/")
            || !page.starts_with("https://commons.wikimedia.org/wiki/")
        {
            continue;
        }
        let Some(bytes) = fetch(thumb, &[], 600_000) else {
            continue;
        };
        let Some(jpeg_base64) = normalized_jpeg(&bytes) else {
            continue;
        };
        refs.push(Reference {
            title: title
                .trim_start_matches("File:")
                .chars()
                .take(100)
                .collect(),
            source: "Wikimedia Commons".into(),
            page: page.chars().take(300).collect(),
            jpeg_base64,
        });
    }
    refs
}

fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.chars().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transformation_reference_queries_preserve_subject_and_split_forms() {
        assert_eq!(
            endpoint_queries(
                "Visualize Optimus Prime's transformation from the truck to the robot."
            ),
            Some(["Optimus Prime truck".into(), "Optimus Prime robot".into()])
        );
        assert_eq!(
            endpoint_queries("Show a bridge transforming from a flat deck to a raised arch"),
            Some(["bridge flat deck".into(), "bridge raised arch".into()])
        );
        assert_eq!(endpoint_queries("Visualize a pickup truck"), None);
    }

    #[test]
    fn endpoint_references_prefer_a_matching_finished_variant() {
        let reference = |title: &str| Reference {
            title: title.into(),
            source: String::new(),
            page: String::new(),
            jpeg_base64: title.into(),
        };
        assert!(!usable_reference_title(
            "Power Up Optimus Prime (Robot Replicas) WIP",
            "Optimus Prime robot"
        ));
        assert!(usable_reference_title(
            "Robot prototype work-in-progress",
            "robot work in progress"
        ));
        let selected = select_endpoint_references(
            vec![reference("G1 truck")],
            vec![reference("G2 robot"), reference("G1 robot")],
        );
        assert_eq!(
            selected
                .iter()
                .map(|image| image.title.as_str())
                .collect::<Vec<_>>(),
            ["G1 truck", "G1 robot"]
        );
        assert_eq!(
            select_endpoint_references(vec![reference("G1 truck")], vec![reference("G2 robot")])
                .len(),
            1
        );
    }

    #[test]
    fn scene_query_drops_presentation_words() {
        assert_eq!(
            query_for_scene("Compact coupe", "Visualize Ultron"),
            "Ultron"
        );
        assert_eq!(query_for_scene("Car", "Visualize electron"), "electron");
        assert_eq!(
            query_for_scene(
                "ISS",
                "Visualize a pickup truck with a distinct cab and four wheels"
            ),
            "pickup truck"
        );
        assert_eq!(
            query_for_scene("", "Create a highly detailed 3D model of a pickup truck"),
            "pickup truck"
        );
        assert!(!plausible_title("A mountain landscape", "Ultron"));
        assert!(plausible_title("Ultron figure", "Ultron"));
        assert!(!plausible_title("Pickup coupe", "pickup truck"));
        assert!(plausible_title(
            "A pickup truck from the side",
            "pickup truck"
        ));
    }
}

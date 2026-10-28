//! Bounded local calculations and Start Menu shortcuts; query text is never
//! passed to a command interpreter or used as an executable path.

const MAX_QUERY_BYTES: usize = 256;
const MAX_APP_ENTRIES: usize = 1_000;
const MAX_MENU_DEPTH: u8 = 6;

#[derive(Clone, Debug)]
pub struct AppEntry {
    pub name: String,
    path: std::path::PathBuf,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
}

impl FileEntry {
    fn validated_target(&self) -> Result<&std::path::Path, LaunchError> {
        let path = std::path::Path::new(&self.path);
        if !path.is_absolute() || !path.exists() {
            return Err(LaunchError);
        }
        Ok(path)
    }

    pub fn launch(&self) -> Result<(), LaunchError> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
        let path = self.validated_target()?;
        let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
        let target: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as isize <= 32 {
            Err(LaunchError)
        } else {
            Ok(())
        }
    }
}

/// Query Windows Search's SystemIndex without opening Explorer. The query is
/// passed through an environment variable, never inserted into PowerShell
/// source as executable text. The worker only returns existing local paths.
pub fn search_files(query: &str) -> Vec<FileEntry> {
    let query = query.trim();
    if query.chars().count() < 2 || query.len() > 128 {
        return Vec::new();
    }
    let script = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$items = New-Object 'System.Collections.Generic.List[object]'
$connection = New-Object System.Data.OleDb.OleDbConnection('Provider=Search.CollatorDSO;Extended Properties="Application=Windows"')
try {
  $connection.Open()
  $term = $env:ISLAND_FILE_QUERY.Replace("'", "''").Replace('[', '[[]').Replace('%', '[%]').Replace('_', '[_]')
  $command = $connection.CreateCommand()
  $command.CommandText = "SELECT TOP 120 System.ItemNameDisplay, System.ItemPathDisplay FROM SYSTEMINDEX WHERE System.ItemNameDisplay LIKE '%" + $term + "%' ORDER BY System.ItemNameDisplay ASC"
  $reader = $command.ExecuteReader()
  while ($reader.Read()) {
    if (-not $reader.IsDBNull(0) -and -not $reader.IsDBNull(1)) {
      $items.Add(@{ name = [string]$reader.GetValue(0); path = [string]$reader.GetValue(1) })
    }
  }
  $reader.Close()
  ConvertTo-Json -Compress -Depth 3 -InputObject @{ available = $true; results = @($items.ToArray()) }
} catch {
  ConvertTo-Json -Compress -Depth 3 -InputObject @{ available = $false; results = @() }
} finally { $connection.Close() }
"#;
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("ISLAND_FILE_QUERY", query)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output();
    #[derive(serde::Deserialize)]
    struct SearchResponse {
        available: bool,
        results: Vec<FileEntry>,
    }
    if let Ok(output) = output
        && let Ok(response) = serde_json::from_slice::<SearchResponse>(&output.stdout)
        && response.available
    {
        let mut results = response
            .results
            .into_iter()
            .filter(|item| std::path::Path::new(&item.path).is_absolute())
            .collect::<Vec<_>>();
        rank_file_results(&mut results, query);
        results.truncate(8);
        return results;
    }
    search_user_folders(query)
}

/// Keep searching useful when Windows Search is disabled or inaccessible.
/// Bounded traversal avoids an unexpected whole-disk walk.
fn search_user_folders(query: &str) -> Vec<FileEntry> {
    let Some(home) = std::env::var_os("USERPROFILE") else {
        return Vec::new();
    };
    let home = std::path::PathBuf::from(home);
    let mut pending = ["Desktop", "Documents", "Downloads", "Pictures"]
        .into_iter()
        .map(|name| (home.join(name), 0_u8))
        .collect::<Vec<_>>();
    let needle = query.to_lowercase();
    let mut visited = 0;
    let mut found = Vec::new();
    while let Some((folder, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > 3_000 {
                break;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.to_lowercase().contains(&needle) {
                found.push(FileEntry {
                    name,
                    path: path.to_string_lossy().into_owned(),
                });
                if found.len() >= 16 {
                    break;
                }
            }
            if depth < 3
                && entry
                    .file_type()
                    .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
            {
                pending.push((path, depth + 1));
            }
        }
        if visited > 3_000 || found.len() >= 16 {
            break;
        }
    }
    rank_file_results(&mut found, query);
    found.truncate(8);
    found
}

fn rank_file_results(results: &mut [FileEntry], query: &str) {
    let needle = query.to_lowercase();
    results.sort_by_key(|item| {
        let name = item.name.to_lowercase();
        let stem = std::path::Path::new(&item.name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(&item.name)
            .to_lowercase();
        let path = item.path.to_lowercase();
        let match_rank = if stem == needle || name == needle {
            0
        } else if stem.starts_with(&needle) {
            1
        } else {
            2
        };
        let location_rank = if path.contains("\\node_modules\\") || path.contains("\\.git\\") {
            5
        } else if path.contains("\\desktop\\") {
            0
        } else if path.contains("\\documents\\") || path.contains("\\downloads\\") {
            1
        } else if path.contains("\\appdata\\") {
            4
        } else {
            2
        };
        (match_rank, location_rank, name.len(), path.len())
    });
}

impl AppEntry {
    /// Return the validated Start Menu shortcut path for Shell icon lookup.
    pub fn icon_path(&self) -> &std::path::Path {
        &self.path
    }

    /// Open only a Start Menu shortcut previously returned by `discover_apps`.
    pub fn launch(&self) -> Result<(), LaunchError> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

        let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
        let path: Vec<u16> = self.path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: both strings are null-terminated UTF-16 buffers that remain
        // alive for the duration of the synchronous ShellExecuteW call.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                path.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as isize <= 32 {
            Err(LaunchError)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaunchError;

impl LaunchError {
    pub const fn user_message(self) -> &'static str {
        "Windows could not open this Start Menu shortcut. Check that the app is still installed."
    }
}

/// Enumerate a bounded set of user/common Start Menu shortcuts. The search
/// never walks the whole disk and never treats query text as a process path.
pub fn discover_apps() -> Vec<AppEntry> {
    let mut entries = std::collections::BTreeMap::<String, AppEntry>::new();
    let mut visited = 0;
    for root in start_menu_roots() {
        scan_menu(&root, 0, &mut visited, &mut entries);
        if visited >= MAX_APP_ENTRIES {
            break;
        }
    }
    let mut apps: Vec<_> = entries.into_values().collect();
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

fn start_menu_roots() -> Vec<std::path::PathBuf> {
    ["APPDATA", "PROGRAMDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from)
        .map(|base| base.join("Microsoft/Windows/Start Menu/Programs"))
        .collect()
}

fn scan_menu(
    directory: &std::path::Path,
    depth: u8,
    visited: &mut usize,
    entries: &mut std::collections::BTreeMap<String, AppEntry>,
) {
    if depth > MAX_MENU_DEPTH || *visited >= MAX_APP_ENTRIES {
        return;
    }
    let Ok(children) = std::fs::read_dir(directory) else {
        return;
    };
    for child in children.flatten() {
        if *visited >= MAX_APP_ENTRIES {
            break;
        }
        *visited += 1;
        let Ok(file_type) = child.file_type() else {
            continue;
        };
        let path = child.path();
        if file_type.is_dir() {
            scan_menu(&path, depth.saturating_add(1), visited, entries);
        } else if file_type.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
        {
            let Some(name) = path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_owned)
            else {
                continue;
            };
            let name = name.trim().to_owned();
            if name.is_empty() {
                continue;
            }
            entries
                .entry(name.to_lowercase())
                .or_insert(AppEntry { name, path });
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryError {
    Empty,
    TooLong,
    InvalidExpression,
    DivisionByZero,
    NonFinite,
    InvalidConversion,
    IncompatibleUnits,
    UnknownUnit,
}

impl QueryError {
    pub fn user_message(self) -> &'static str {
        match self {
            Self::Empty => "Enter a calculation or conversion.",
            Self::TooLong => "Keep the query under 256 bytes.",
            Self::InvalidExpression => "Use numbers, parentheses, and + − × ÷ operators.",
            Self::DivisionByZero => "Division by zero is not defined.",
            Self::NonFinite => "The result is outside the supported numeric range.",
            Self::InvalidConversion => "Use the form: 5 km to mi.",
            Self::IncompatibleUnits => "Those units measure different quantities.",
            Self::UnknownUnit => "That unit is not supported.",
        }
    }
}

pub fn evaluate(query: &str) -> Result<String, QueryError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(QueryError::Empty);
    }
    if query.len() > MAX_QUERY_BYTES {
        return Err(QueryError::TooLong);
    }
    if let Some(expression) = query.strip_prefix("=") {
        return calculate(expression.trim());
    }
    if let Some(expression) = query
        .strip_prefix("calc ")
        .or_else(|| query.strip_prefix("calc:"))
    {
        return calculate(expression.trim());
    }

    if query.split_whitespace().count() == 4 {
        return convert(query);
    }
    calculate(query)
}

fn calculate(expression: &str) -> Result<String, QueryError> {
    let mut parser = Parser::new(expression.as_bytes());
    let value = parser.parse_sum()?;
    parser.skip_whitespace();
    if parser.position != parser.input.len() {
        return Err(QueryError::InvalidExpression);
    }
    finite(value)?;
    Ok(format_number(value))
}

fn convert(query: &str) -> Result<String, QueryError> {
    let parts: Vec<_> = query.split_whitespace().collect();
    if parts.len() != 4 || !parts[2].eq_ignore_ascii_case("to") {
        return Err(QueryError::InvalidConversion);
    }
    let value = parts[0]
        .parse::<f64>()
        .map_err(|_| QueryError::InvalidConversion)?;
    finite(value)?;
    let source = Unit::parse(parts[1]).ok_or(QueryError::UnknownUnit)?;
    let target = Unit::parse(parts[3]).ok_or(QueryError::UnknownUnit)?;
    if source.dimension != target.dimension {
        return Err(QueryError::IncompatibleUnits);
    }
    let converted = match source.dimension {
        Dimension::Length | Dimension::Mass => value * source.factor / target.factor,
        Dimension::Temperature => target.convert_from_celsius(source.to_celsius(value)),
    };
    finite(converted)?;
    Ok(format!("{} {}", format_number(converted), target.canonical))
}

fn finite(value: f64) -> Result<f64, QueryError> {
    value
        .is_finite()
        .then_some(value)
        .ok_or(QueryError::NonFinite)
}

fn format_number(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    let mut output = format!("{value:.6}");
    while output.ends_with('0') {
        output.pop();
    }
    if output.ends_with('.') {
        output.pop();
    }
    output
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dimension {
    Length,
    Mass,
    Temperature,
}

#[derive(Clone, Copy)]
struct Unit {
    dimension: Dimension,
    canonical: &'static str,
    factor: f64,
    temperature: TemperatureScale,
}

#[derive(Clone, Copy)]
enum TemperatureScale {
    None,
    Celsius,
    Fahrenheit,
    Kelvin,
}

impl Unit {
    fn parse(value: &str) -> Option<Self> {
        let (dimension, canonical, factor, temperature) = match value.to_ascii_lowercase().as_str()
        {
            "mm" => (Dimension::Length, "mm", 0.001, TemperatureScale::None),
            "cm" => (Dimension::Length, "cm", 0.01, TemperatureScale::None),
            "m" => (Dimension::Length, "m", 1.0, TemperatureScale::None),
            "km" => (Dimension::Length, "km", 1_000.0, TemperatureScale::None),
            "in" => (Dimension::Length, "in", 0.0254, TemperatureScale::None),
            "ft" => (Dimension::Length, "ft", 0.3048, TemperatureScale::None),
            "mi" => (Dimension::Length, "mi", 1_609.344, TemperatureScale::None),
            "g" => (Dimension::Mass, "g", 0.001, TemperatureScale::None),
            "kg" => (Dimension::Mass, "kg", 1.0, TemperatureScale::None),
            "lb" => (Dimension::Mass, "lb", 0.453_592_37, TemperatureScale::None),
            "oz" => (
                Dimension::Mass,
                "oz",
                0.028_349_523_125,
                TemperatureScale::None,
            ),
            "c" | "°c" => (Dimension::Temperature, "°C", 1.0, TemperatureScale::Celsius),
            "f" | "°f" => (
                Dimension::Temperature,
                "°F",
                1.0,
                TemperatureScale::Fahrenheit,
            ),
            "k" => (Dimension::Temperature, "K", 1.0, TemperatureScale::Kelvin),
            _ => return None,
        };
        Some(Self {
            dimension,
            canonical,
            factor,
            temperature,
        })
    }

    fn to_celsius(self, value: f64) -> f64 {
        match self.temperature {
            TemperatureScale::Celsius => value,
            TemperatureScale::Fahrenheit => (value - 32.0) * (5.0 / 9.0),
            TemperatureScale::Kelvin => value - 273.15,
            TemperatureScale::None => value,
        }
    }

    fn convert_from_celsius(self, value: f64) -> f64 {
        match self.temperature {
            TemperatureScale::Celsius => value,
            TemperatureScale::Fahrenheit => value * (9.0 / 5.0) + 32.0,
            TemperatureScale::Kelvin => value + 273.15,
            TemperatureScale::None => value,
        }
    }
}

struct Parser<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, position: 0 }
    }

    fn parse_sum(&mut self) -> Result<f64, QueryError> {
        let mut value = self.parse_product()?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'+') => {
                    self.position += 1;
                    value += self.parse_product()?;
                }
                Some(b'-') => {
                    self.position += 1;
                    value -= self.parse_product()?;
                }
                _ => return Ok(value),
            }
            finite(value)?;
        }
    }

    fn parse_product(&mut self) -> Result<f64, QueryError> {
        let mut value = self.parse_unary()?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'*') | Some(b'x') | Some(b'X') => {
                    self.position += 1;
                    value *= self.parse_unary()?;
                }
                Some(b'/') => {
                    self.position += 1;
                    let divisor = self.parse_unary()?;
                    if divisor == 0.0 {
                        return Err(QueryError::DivisionByZero);
                    }
                    value /= divisor;
                }
                _ => return Ok(value),
            }
            finite(value)?;
        }
    }

    fn parse_unary(&mut self) -> Result<f64, QueryError> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'+') => {
                self.position += 1;
                self.parse_unary()
            }
            Some(b'-') => {
                self.position += 1;
                Ok(-self.parse_unary()?)
            }
            _ => self.parse_atom(),
        }
    }

    fn parse_atom(&mut self) -> Result<f64, QueryError> {
        self.skip_whitespace();
        if self.peek() == Some(b'(') {
            self.position += 1;
            let value = self.parse_sum()?;
            self.skip_whitespace();
            if self.peek() != Some(b')') {
                return Err(QueryError::InvalidExpression);
            }
            self.position += 1;
            return Ok(value);
        }

        let start = self.position;
        let mut saw_digit = false;
        let mut saw_dot = false;
        while let Some(byte) = self.peek() {
            match byte {
                b'0'..=b'9' => {
                    saw_digit = true;
                    self.position += 1;
                }
                b'.' if !saw_dot => {
                    saw_dot = true;
                    self.position += 1;
                }
                _ => break,
            }
        }
        if !saw_digit {
            return Err(QueryError::InvalidExpression);
        }
        let text = std::str::from_utf8(&self.input[start..self.position])
            .map_err(|_| QueryError::InvalidExpression)?;
        let value = text
            .parse::<f64>()
            .map_err(|_| QueryError::InvalidExpression)?;
        finite(value)
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.position).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::{QueryError, evaluate};

    #[test]
    fn calculator_obeys_precedence_unary_and_parentheses() {
        assert_eq!(evaluate("2 + 3 * (4 - 1)"), Ok("11".into()));
        assert_eq!(evaluate("calc: -2.5 * 4"), Ok("-10".into()));
        assert_eq!(evaluate("= 1 / 4"), Ok("0.25".into()));
    }

    #[test]
    fn calculator_rejects_invalid_or_unbounded_queries() {
        assert_eq!(evaluate("1 / 0"), Err(QueryError::DivisionByZero));
        assert_eq!(evaluate("1 + unknown"), Err(QueryError::InvalidExpression));
        assert_eq!(evaluate("1; calc"), Err(QueryError::InvalidExpression));
        assert_eq!(evaluate(&"1".repeat(257)), Err(QueryError::TooLong));
        assert_eq!(evaluate(""), Err(QueryError::Empty));
    }

    #[test]
    fn converter_supports_whitelisted_units_and_temperature() {
        let miles = evaluate("5 km to mi").unwrap();
        assert!(miles.starts_with("3.106856 mi"));
        assert_eq!(evaluate("0 c to f"), Ok("32 °F".into()));
        assert_eq!(evaluate("1 kg to g"), Ok("1000 g".into()));
        assert_eq!(evaluate("2 km to kg"), Err(QueryError::IncompatibleUnits));
        assert_eq!(evaluate("1 furlong to km"), Err(QueryError::UnknownUnit));
    }

    #[test]
    fn app_catalog_only_keeps_case_insensitive_unique_shortcuts() {
        let unique = format!(
            "island-start-menu-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(root.join("Tools")).unwrap();
        std::fs::write(root.join("Editor.lnk"), b"").unwrap();
        std::fs::write(root.join("editor.LNK"), b"").unwrap();
        std::fs::write(root.join("Tools/Terminal.lnk"), b"").unwrap();
        std::fs::write(root.join("Not an app.exe"), b"").unwrap();
        std::fs::write(root.join("Website.url"), b"").unwrap();

        let mut visited = 0;
        let mut entries = std::collections::BTreeMap::new();
        super::scan_menu(&root, 0, &mut visited, &mut entries);
        let mut names: Vec<_> = entries.into_values().map(|entry| entry.name).collect();
        names.sort();
        assert_eq!(names, ["Editor", "Terminal"]);
        assert!(visited <= super::MAX_APP_ENTRIES);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_launch_target_requires_an_existing_absolute_path() {
        let unique = format!(
            "island-project-folder-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("not-a-directory.txt");
        std::fs::write(&file, b"fixture").unwrap();

        let entry = |path: &std::path::Path| super::FileEntry {
            name: "fixture".into(),
            path: path.to_string_lossy().into_owned(),
        };
        // Validation must not launch Explorer or another application during tests.
        assert!(entry(&root).validated_target().is_ok());
        assert!(entry(&file).validated_target().is_ok());
        assert_eq!(
            entry(std::path::Path::new("relative")).validated_target(),
            Err(super::LaunchError)
        );
        assert_eq!(
            entry(&root.join("missing")).validated_target(),
            Err(super::LaunchError)
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}

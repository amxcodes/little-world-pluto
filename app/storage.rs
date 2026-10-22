//! App-owned preferences. Codex sessions and credentials never enter this DB.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use island_desktop::motion::{HOVER_CLOSE_DELAY, HOVER_OPEN_DELAY};
use island_desktop::visual::ShapeMode;
use rusqlite::ffi::ErrorCode;
use rusqlite::{Connection, OptionalExtension, params};

const SCHEMA_VERSION: i64 = 11;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StoredTheme {
    #[default]
    System,
    Dark,
    Light,
}

impl StoredTheme {
    fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::System),
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StoredAccentGradient {
    #[default]
    Lime,
    Ice,
    Rose,
    Violet,
}

impl StoredAccentGradient {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lime => "lime",
            Self::Ice => "ice",
            Self::Rose => "rose",
            Self::Violet => "violet",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "lime" => Some(Self::Lime),
            "ice" => Some(Self::Ice),
            "rose" => Some(Self::Rose),
            "violet" => Some(Self::Violet),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StoredMediaCardStyle {
    #[default]
    Standard,
    Backdrop,
    Studio,
}

impl StoredMediaCardStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Backdrop => "backdrop",
            Self::Studio => "studio",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "standard" => Some(Self::Standard),
            "backdrop" => Some(Self::Backdrop),
            "studio" => Some(Self::Studio),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoredSettings {
    pub theme: StoredTheme,
    pub accent_gradient: StoredAccentGradient,
    pub media_card_style: StoredMediaCardStyle,
    pub shape: ShapeMode,
    pub zoom: f32,
    pub reduced_motion: bool,
    pub hover_open_delay_ms: u32,
    pub hover_close_delay_ms: u32,
    pub click_to_open: bool,
    pub close_on_outside_click: bool,
    pub folded_transparency_percent: u8,
    pub open_transparency_percent: u8,
    pub clock_indicator_enabled: bool,
    pub battery_indicator_enabled: bool,
    pub privacy_indicator_enabled: bool,
    pub hide_in_fullscreen: bool,
    pub exclude_from_screen_capture: bool,
    pub shortcuts: crate::shortcuts::ShortcutSet,
}

impl Default for StoredSettings {
    fn default() -> Self {
        Self {
            theme: StoredTheme::System,
            accent_gradient: StoredAccentGradient::Lime,
            media_card_style: StoredMediaCardStyle::Standard,
            shape: ShapeMode::Notch,
            zoom: 1.0,
            reduced_motion: false,
            hover_open_delay_ms: HOVER_OPEN_DELAY.as_millis() as u32,
            hover_close_delay_ms: HOVER_CLOSE_DELAY.as_millis() as u32,
            click_to_open: false,
            close_on_outside_click: false,
            folded_transparency_percent: 0,
            open_transparency_percent: 0,
            clock_indicator_enabled: true,
            battery_indicator_enabled: true,
            privacy_indicator_enabled: true,
            hide_in_fullscreen: true,
            exclude_from_screen_capture: false,
            shortcuts: crate::shortcuts::ShortcutSet::default(),
        }
    }
}

impl StoredSettings {
    fn valid(self) -> bool {
        self.zoom.is_finite()
            && (0.8..=1.5).contains(&self.zoom)
            && self.hover_open_delay_ms <= 600
            && (100..=1_500).contains(&self.hover_close_delay_ms)
            && self.folded_transparency_percent <= 100
            && self.open_transparency_percent <= 100
            && self.shortcuts.is_valid()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageError {
    Unavailable,
    InvalidSettings,
    UnsupportedSchema,
}

pub struct SettingsStore {
    connection: Connection,
}

impl SettingsStore {
    pub fn set_theme(&mut self, theme: StoredTheme) -> Result<(), StorageError> {
        let changed = self
            .connection
            .execute(
                "UPDATE preferences SET theme=?1 WHERE singleton=1",
                [theme.as_str()],
            )
            .map_err(|_| StorageError::Unavailable)?;
        if changed == 0 {
            let mut settings = StoredSettings::default();
            settings.theme = theme;
            self.save(settings)?;
        }
        Ok(())
    }

    pub fn open_default() -> Result<Self, StorageError> {
        let local = std::env::var_os("LOCALAPPDATA").ok_or(StorageError::Unavailable)?;
        Self::open(Path::new(&local).join("Island").join("settings.sqlite3"))
    }

    pub fn open(path: PathBuf) -> Result<Self, StorageError> {
        let parent = path.parent().ok_or(StorageError::Unavailable)?;
        fs::create_dir_all(parent).map_err(|_| StorageError::Unavailable)?;

        let connection = Connection::open(&path).map_err(|_| StorageError::Unavailable)?;
        match check_integrity(&connection) {
            Ok(()) => {}
            Err(IntegrityError::Corrupt) => {
                drop(connection);
                move_corrupt_database(&path)?;
                let replacement = Self::from_connection(
                    Connection::open(&path).map_err(|_| StorageError::Unavailable)?,
                )?;
                replacement
                    .connection
                    .execute_batch("PRAGMA journal_mode=WAL;")
                    .map_err(|_| StorageError::Unavailable)?;
                return Ok(replacement);
            }
            Err(IntegrityError::Unavailable) => return Err(StorageError::Unavailable),
        }
        let store = Self::from_connection(connection)?;
        store
            .connection
            .execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|_| StorageError::Unavailable)?;
        Ok(store)
    }

    fn from_connection(mut connection: Connection) -> Result<Self, StorageError> {
        connection
            .busy_timeout(Duration::from_millis(500))
            .map_err(|_| StorageError::Unavailable)?;
        migrate(&mut connection)?;
        Ok(Self { connection })
    }

    pub fn load(&self) -> Result<StoredSettings, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT theme, shape, zoom, reduced_motion, hover_open_delay_ms, hover_close_delay_ms, folded_transparency_percent, open_transparency_percent, battery_indicator_enabled, clock_indicator_enabled, shortcuts_json, privacy_indicator_enabled, hide_in_fullscreen, accent_gradient, media_card_style, exclude_from_screen_capture, click_to_open, close_on_outside_click FROM preferences WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, f32>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, String>(10)?,
                        row.get::<_, i64>(11)?,
                        row.get::<_, i64>(12)?,
                        row.get::<_, String>(13)?,
                        row.get::<_, String>(14)?,
                        row.get::<_, i64>(15)?,
                        row.get::<_, i64>(16)?,
                        row.get::<_, i64>(17)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| StorageError::Unavailable)?;
        let Some((
            theme,
            shape,
            zoom,
            reduced_motion,
            open_ms,
            close_ms,
            folded_alpha,
            open_alpha,
            battery_indicator_enabled,
            clock_indicator_enabled,
            shortcuts_json,
            privacy_indicator_enabled,
            hide_in_fullscreen,
            accent_gradient,
            media_card_style,
            exclude_from_screen_capture,
            click_to_open,
            close_on_outside_click,
        )) = row
        else {
            return Ok(StoredSettings::default());
        };
        let settings = StoredSettings {
            theme: StoredTheme::parse(&theme).ok_or(StorageError::InvalidSettings)?,
            accent_gradient: StoredAccentGradient::parse(&accent_gradient)
                .ok_or(StorageError::InvalidSettings)?,
            media_card_style: StoredMediaCardStyle::parse(&media_card_style)
                .ok_or(StorageError::InvalidSettings)?,
            shape: match shape.as_str() {
                "notch" => ShapeMode::Notch,
                "capsule" => ShapeMode::Capsule,
                _ => return Err(StorageError::InvalidSettings),
            },
            zoom,
            reduced_motion: match reduced_motion {
                0 => false,
                1 => true,
                _ => return Err(StorageError::InvalidSettings),
            },
            hover_open_delay_ms: u32::try_from(open_ms)
                .map_err(|_| StorageError::InvalidSettings)?,
            hover_close_delay_ms: u32::try_from(close_ms)
                .map_err(|_| StorageError::InvalidSettings)?,
            click_to_open: match click_to_open {
                0 => false,
                1 => true,
                _ => return Err(StorageError::InvalidSettings),
            },
            close_on_outside_click: match close_on_outside_click {
                0 => false,
                1 => true,
                _ => return Err(StorageError::InvalidSettings),
            },
            folded_transparency_percent: u8::try_from(folded_alpha)
                .map_err(|_| StorageError::InvalidSettings)?,
            open_transparency_percent: u8::try_from(open_alpha)
                .map_err(|_| StorageError::InvalidSettings)?,
            battery_indicator_enabled: match battery_indicator_enabled {
                0 => false,
                1 => true,
                _ => return Err(StorageError::InvalidSettings),
            },
            clock_indicator_enabled: match clock_indicator_enabled {
                0 => false,
                1 => true,
                _ => return Err(StorageError::InvalidSettings),
            },
            privacy_indicator_enabled: match privacy_indicator_enabled {
                0 => false,
                1 => true,
                _ => return Err(StorageError::InvalidSettings),
            },
            hide_in_fullscreen: match hide_in_fullscreen {
                0 => false,
                1 => true,
                _ => return Err(StorageError::InvalidSettings),
            },
            exclude_from_screen_capture: match exclude_from_screen_capture {
                0 => false,
                1 => true,
                _ => return Err(StorageError::InvalidSettings),
            },
            shortcuts: if shortcuts_json.is_empty() {
                crate::shortcuts::ShortcutSet::default()
            } else {
                crate::shortcuts::ShortcutSet::decode_json(&shortcuts_json)
                    .ok_or(StorageError::InvalidSettings)?
            },
        };
        settings
            .valid()
            .then_some(settings)
            .ok_or(StorageError::InvalidSettings)
    }

    pub fn save(&mut self, settings: StoredSettings) -> Result<(), StorageError> {
        if !settings.valid() {
            return Err(StorageError::InvalidSettings);
        }
        let shape = match settings.shape {
            ShapeMode::Notch => "notch",
            ShapeMode::Capsule => "capsule",
        };
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| StorageError::Unavailable)?;
        transaction
            .execute(
                "INSERT INTO preferences (singleton, theme, shape, zoom, reduced_motion, hover_open_delay_ms, hover_close_delay_ms, folded_transparency_percent, open_transparency_percent, battery_indicator_enabled, clock_indicator_enabled, shortcuts_json, privacy_indicator_enabled, hide_in_fullscreen, accent_gradient, media_card_style, exclude_from_screen_capture, click_to_open, close_on_outside_click)
                 VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
                 ON CONFLICT(singleton) DO UPDATE SET theme=excluded.theme, shape=excluded.shape,
                    zoom=excluded.zoom, reduced_motion=excluded.reduced_motion,
                    hover_open_delay_ms=excluded.hover_open_delay_ms,
                    hover_close_delay_ms=excluded.hover_close_delay_ms,
                    folded_transparency_percent=excluded.folded_transparency_percent,
                    open_transparency_percent=excluded.open_transparency_percent,
                    battery_indicator_enabled=excluded.battery_indicator_enabled,
                    clock_indicator_enabled=excluded.clock_indicator_enabled,
                    shortcuts_json=excluded.shortcuts_json,
                    privacy_indicator_enabled=excluded.privacy_indicator_enabled,
                    hide_in_fullscreen=excluded.hide_in_fullscreen,
                    accent_gradient=excluded.accent_gradient,
                    media_card_style=excluded.media_card_style,
                    exclude_from_screen_capture=excluded.exclude_from_screen_capture,
                    click_to_open=excluded.click_to_open,
                    close_on_outside_click=excluded.close_on_outside_click",
                params![
                    settings.theme.as_str(),
                    shape,
                    settings.zoom,
                    i64::from(settings.reduced_motion),
                    i64::from(settings.hover_open_delay_ms),
                    i64::from(settings.hover_close_delay_ms),
                    i64::from(settings.folded_transparency_percent),
                    i64::from(settings.open_transparency_percent),
                    i64::from(settings.battery_indicator_enabled),
                    i64::from(settings.clock_indicator_enabled),
                    settings.shortcuts.encode_json(),
                    i64::from(settings.privacy_indicator_enabled),
                    i64::from(settings.hide_in_fullscreen),
                    settings.accent_gradient.as_str(),
                    settings.media_card_style.as_str(),
                    i64::from(settings.exclude_from_screen_capture),
                    i64::from(settings.click_to_open),
                    i64::from(settings.close_on_outside_click)
                ],
            )
            .map_err(|_| StorageError::Unavailable)?;
        transaction.commit().map_err(|_| StorageError::Unavailable)
    }
}

fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| StorageError::Unavailable)?;
    if !(0..=SCHEMA_VERSION).contains(&version) {
        return Err(StorageError::UnsupportedSchema);
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    let transaction = connection
        .transaction()
        .map_err(|_| StorageError::Unavailable)?;
    if version == 0 {
        transaction
            .execute_batch(
            "CREATE TABLE preferences (
                singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                theme TEXT NOT NULL CHECK(theme IN ('system', 'dark', 'light')),
                shape TEXT NOT NULL CHECK(shape IN ('notch', 'capsule')),
                zoom REAL NOT NULL CHECK(zoom >= 0.8 AND zoom <= 1.5),
                reduced_motion INTEGER NOT NULL CHECK(reduced_motion IN (0, 1)),
                hover_open_delay_ms INTEGER NOT NULL CHECK(hover_open_delay_ms BETWEEN 0 AND 600),
                hover_close_delay_ms INTEGER NOT NULL CHECK(hover_close_delay_ms BETWEEN 100 AND 1500)
            );
            PRAGMA user_version = 1;",
        )
            .map_err(|_| StorageError::Unavailable)?;
    }
    if version <= 1 {
        transaction
            .execute_batch(
                "ALTER TABLE preferences ADD COLUMN folded_transparency_percent INTEGER NOT NULL DEFAULT 0 CHECK(folded_transparency_percent BETWEEN 0 AND 100);
                 ALTER TABLE preferences ADD COLUMN open_transparency_percent INTEGER NOT NULL DEFAULT 0 CHECK(open_transparency_percent BETWEEN 0 AND 100);
                 PRAGMA user_version = 2;",
            )
            .map_err(|_| StorageError::Unavailable)?;
    }
    if version <= 2 {
        transaction
            .execute_batch(
                "CREATE TABLE pinned_codex_threads (
                    thread_id TEXT PRIMARY KEY CHECK(length(thread_id) BETWEEN 1 AND 256)
                ) WITHOUT ROWID;
                PRAGMA user_version = 3;",
            )
            .map_err(|_| StorageError::Unavailable)?;
    }
    if version <= 3 {
        transaction
            .execute_batch(
                "ALTER TABLE preferences ADD COLUMN battery_indicator_enabled INTEGER NOT NULL DEFAULT 1 CHECK(battery_indicator_enabled IN (0, 1));
                 PRAGMA user_version = 4;",
            )
            .map_err(|_| StorageError::Unavailable)?;
    }
    if version <= 4 {
        transaction
            .execute_batch(
                "ALTER TABLE preferences ADD COLUMN clock_indicator_enabled INTEGER NOT NULL DEFAULT 1 CHECK(clock_indicator_enabled IN (0, 1));
                 PRAGMA user_version = 5;",
            )
            .map_err(|_| StorageError::Unavailable)?;
    }
    if version <= 5 {
        transaction
            .execute_batch(
                "ALTER TABLE preferences ADD COLUMN shortcuts_json TEXT NOT NULL DEFAULT '';
                 PRAGMA user_version = 6;",
            )
            .map_err(|_| StorageError::Unavailable)?;
    }
    if version <= 6 {
        transaction
            .execute_batch(
                "ALTER TABLE preferences ADD COLUMN privacy_indicator_enabled INTEGER NOT NULL DEFAULT 1 CHECK(privacy_indicator_enabled IN (0, 1));
                 PRAGMA user_version = 7;",
            )
            .map_err(|_| StorageError::Unavailable)?;
    }
    if version <= 7 {
        transaction
            .execute_batch(
                "ALTER TABLE preferences ADD COLUMN hide_in_fullscreen INTEGER NOT NULL DEFAULT 1 CHECK(hide_in_fullscreen IN (0, 1));
                 PRAGMA user_version = 8;",
            )
            .map_err(|_| StorageError::Unavailable)?;
    }
    if version <= 8 {
        transaction
            .execute_batch(
                "ALTER TABLE preferences ADD COLUMN accent_gradient TEXT NOT NULL DEFAULT 'lime' CHECK(accent_gradient IN ('lime', 'ice', 'rose', 'violet'));
                 ALTER TABLE preferences ADD COLUMN media_card_style TEXT NOT NULL DEFAULT 'standard' CHECK(media_card_style IN ('standard', 'backdrop'));
                 PRAGMA user_version = 9;",
            )
            .map_err(|_| StorageError::Unavailable)?;
    }
    if version <= 9 {
        transaction
            .execute_batch(
                "ALTER TABLE preferences ADD COLUMN exclude_from_screen_capture INTEGER NOT NULL DEFAULT 0 CHECK(exclude_from_screen_capture IN (0, 1));
                 PRAGMA user_version = 10;",
            )
            .map_err(|_| StorageError::Unavailable)?;
    }
    if version <= 10 {
        transaction
            .execute_batch(
                "ALTER TABLE preferences ADD COLUMN click_to_open INTEGER NOT NULL DEFAULT 0 CHECK(click_to_open IN (0, 1));
                 ALTER TABLE preferences ADD COLUMN close_on_outside_click INTEGER NOT NULL DEFAULT 0 CHECK(close_on_outside_click IN (0, 1));
                 PRAGMA user_version = 11;",
            )
            .map_err(|_| StorageError::Unavailable)?;
    }
    transaction.commit().map_err(|_| StorageError::Unavailable)
}

enum IntegrityError {
    Corrupt,
    Unavailable,
}

fn check_integrity(connection: &Connection) -> Result<(), IntegrityError> {
    match connection.query_row("PRAGMA quick_check(1)", [], |row| row.get::<_, String>(0)) {
        Ok(result) if result == "ok" => Ok(()),
        Ok(_) => Err(IntegrityError::Corrupt),
        Err(rusqlite::Error::SqliteFailure(error, _))
            if matches!(
                error.code,
                ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase
            ) =>
        {
            Err(IntegrityError::Corrupt)
        }
        Err(_) => Err(IntegrityError::Unavailable),
    }
}

fn move_corrupt_database(path: &Path) -> Result<PathBuf, StorageError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StorageError::Unavailable)?
        .as_nanos();
    let mut backup = path.as_os_str().to_os_string();
    backup.push(format!(".corrupt-{stamp}"));
    let backup = PathBuf::from(backup);
    fs::rename(path, &backup).map_err(|_| StorageError::Unavailable)?;
    for suffix in ["-wal", "-shm"] {
        let mut source = path.as_os_str().to_os_string();
        source.push(suffix);
        let mut destination = backup.as_os_str().to_os_string();
        destination.push(suffix);
        let source = PathBuf::from(source);
        if source.exists() {
            fs::rename(source, PathBuf::from(destination))
                .map_err(|_| StorageError::Unavailable)?;
        }
    }
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory() -> SettingsStore {
        SettingsStore::from_connection(Connection::open_in_memory().unwrap()).unwrap()
    }

    #[test]
    fn migration_round_trip_reset_and_invalid_write() {
        let mut store = memory();
        assert_eq!(store.load().unwrap(), StoredSettings::default());
        let shortcuts = crate::shortcuts::ShortcutSet::default();
        let value = StoredSettings {
            theme: StoredTheme::Light,
            accent_gradient: StoredAccentGradient::Rose,
            media_card_style: StoredMediaCardStyle::Backdrop,
            shape: ShapeMode::Capsule,
            zoom: 1.2,
            reduced_motion: true,
            hover_open_delay_ms: 150,
            hover_close_delay_ms: 500,
            click_to_open: true,
            close_on_outside_click: true,
            folded_transparency_percent: 25,
            open_transparency_percent: 40,
            battery_indicator_enabled: false,
            clock_indicator_enabled: false,
            privacy_indicator_enabled: false,
            hide_in_fullscreen: false,
            exclude_from_screen_capture: true,
            shortcuts,
        };
        store.save(value).unwrap();
        assert_eq!(store.load().unwrap(), value);
        let invalid = StoredSettings {
            zoom: f32::NAN,
            ..value
        };
        assert_eq!(store.save(invalid), Err(StorageError::InvalidSettings));
        assert_eq!(store.load().unwrap(), value);
        store.save(StoredSettings::default()).unwrap();
        assert_eq!(store.load().unwrap(), StoredSettings::default());
    }

    #[test]
    fn failed_migration_rolls_back_schema_version() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE preferences (old_value TEXT);")
            .unwrap();
        assert_eq!(migrate(&mut connection), Err(StorageError::Unavailable));
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 0);
    }

    #[test]
    fn version_one_preferences_gain_independent_transparency() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE preferences (
                    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                    theme TEXT NOT NULL, shape TEXT NOT NULL, zoom REAL NOT NULL,
                    reduced_motion INTEGER NOT NULL, hover_open_delay_ms INTEGER NOT NULL,
                    hover_close_delay_ms INTEGER NOT NULL
                );
                INSERT INTO preferences VALUES (1, 'dark', 'capsule', 1.2, 1, 150, 350);
                PRAGMA user_version = 1;",
            )
            .unwrap();
        migrate(&mut connection).unwrap();
        let store = SettingsStore { connection };
        let settings = store.load().unwrap();
        assert_eq!(settings.shape, ShapeMode::Capsule);
        assert_eq!(settings.theme, StoredTheme::Dark);
        assert_eq!(settings.folded_transparency_percent, 0);
        assert_eq!(settings.open_transparency_percent, 0);
        let version: i64 = store
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert!(settings.battery_indicator_enabled);
        assert!(settings.clock_indicator_enabled);
        assert!(settings.privacy_indicator_enabled);
        assert_eq!(version, 10);
    }

    #[test]
    fn version_six_preferences_gain_privacy_and_fullscreen_settings() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE preferences (
                    singleton INTEGER PRIMARY KEY,
                    theme TEXT NOT NULL, shape TEXT NOT NULL, zoom REAL NOT NULL,
                    reduced_motion INTEGER NOT NULL, hover_open_delay_ms INTEGER NOT NULL,
                    hover_close_delay_ms INTEGER NOT NULL,
                    folded_transparency_percent INTEGER NOT NULL,
                    open_transparency_percent INTEGER NOT NULL,
                    battery_indicator_enabled INTEGER NOT NULL,
                    clock_indicator_enabled INTEGER NOT NULL,
                    shortcuts_json TEXT NOT NULL
                );
                INSERT INTO preferences VALUES (1, 'dark', 'notch', 1.0, 0, 150, 350, 0, 0, 1, 1, '');
                PRAGMA user_version = 6;",
            )
            .unwrap();
        migrate(&mut connection).unwrap();
        let store = SettingsStore { connection };
        let settings = store.load().unwrap();
        assert!(settings.privacy_indicator_enabled);
        assert!(settings.hide_in_fullscreen);
    }

    #[test]
    fn version_seven_preferences_gain_fullscreen_suppression_by_default() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE preferences (
                    singleton INTEGER PRIMARY KEY,
                    theme TEXT NOT NULL, shape TEXT NOT NULL, zoom REAL NOT NULL,
                    reduced_motion INTEGER NOT NULL, hover_open_delay_ms INTEGER NOT NULL,
                    hover_close_delay_ms INTEGER NOT NULL,
                    folded_transparency_percent INTEGER NOT NULL,
                    open_transparency_percent INTEGER NOT NULL,
                    battery_indicator_enabled INTEGER NOT NULL,
                    clock_indicator_enabled INTEGER NOT NULL,
                    shortcuts_json TEXT NOT NULL,
                    privacy_indicator_enabled INTEGER NOT NULL
                );
                INSERT INTO preferences VALUES (1, 'dark', 'notch', 1.0, 0, 0, 350, 0, 0, 1, 1, '', 1);
                PRAGMA user_version = 7;",
            )
            .unwrap();
        migrate(&mut connection).unwrap();
        let store = SettingsStore { connection };
        let settings = store.load().unwrap();
        assert!(settings.hide_in_fullscreen);
        assert_eq!(
            store
                .connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            10
        );
    }

    #[test]
    fn corrupt_database_is_preserved_before_fresh_migration() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "island-settings-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("settings.sqlite3");
        fs::write(&path, b"this is not a SQLite database").unwrap();
        let store = SettingsStore::open(path.clone()).unwrap();
        let backup = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|candidate| {
                candidate.file_name().is_some_and(|name| {
                    let name = name.to_string_lossy();
                    name.contains(".corrupt-") && !name.ends_with("-wal") && !name.ends_with("-shm")
                })
            })
            .unwrap();
        assert_eq!(fs::read(&backup).unwrap(), b"this is not a SQLite database");
        assert_eq!(store.load().unwrap(), StoredSettings::default());
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }
}

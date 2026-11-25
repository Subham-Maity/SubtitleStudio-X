//! SQLite persistence layer (`rusqlite` bundled, WAL mode, foreign keys ON).

use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use subtitle_render::{
    AnimationIn, AnimationOut, EasingKind, KaraokeMode, StrokePosition,
};
use uuid::Uuid;

use crate::error::CoreError;
use crate::models::{
    ExportFormat, ExportJob, ExportMode, ExportQuality, ExportStatus, PipelineStatus,
    SubtitleCue, SubtitleStyle, TranscriptWord, UpdateStylePatch, VideoMetadata, VideoProject,
};

/// Cue insertion tuple `(text, start_ms, end_ms, cue_order, word_start_idx, word_end_idx)`.
pub type CueInsertRow = (String, i64, i64, i32, Option<usize>, Option<usize>);

/// Repository trait decoupling domain logic from SQLite connection details.
pub trait StudioRepository: Send + Sync {
    fn create_project(
        &self,
        original_filename: &str,
        storage_path: &str,
        metadata: VideoMetadata,
        language: Option<&str>,
    ) -> Result<VideoProject, CoreError>;

    fn list_projects(&self) -> Result<Vec<VideoProject>, CoreError>;
    fn get_project(&self, id: &str) -> Result<Option<VideoProject>, CoreError>;
    fn update_project_status(
        &self,
        id: &str,
        status: PipelineStatus,
        error_message: Option<&str>,
    ) -> Result<(), CoreError>;
    fn update_project_language(&self, id: &str, language: Option<&str>) -> Result<(), CoreError>;
    fn delete_project(&self, id: &str) -> Result<(), CoreError>;

    fn replace_words(
        &self,
        video_project_id: &str,
        words: &[(String, i64, i64)],
    ) -> Result<Vec<TranscriptWord>, CoreError>;

    fn replace_cues(
        &self,
        video_project_id: &str,
        cues: &[CueInsertRow],
    ) -> Result<Vec<SubtitleCue>, CoreError>;

    fn create_cue(
        &self,
        video_project_id: &str,
        text: &str,
        start_ms: i64,
        end_ms: i64,
        color_hex: Option<&str>,
    ) -> Result<SubtitleCue, CoreError>;

    fn update_cue(
        &self,
        cue_id: &str,
        text: Option<&str>,
        start_ms: Option<i64>,
        end_ms: Option<i64>,
        color_hex: Option<Option<&str>>,
    ) -> Result<SubtitleCue, CoreError>;

    fn delete_cue(&self, cue_id: &str) -> Result<(), CoreError>;

    fn update_style(
        &self,
        video_project_id: &str,
        patch: &UpdateStylePatch,
    ) -> Result<SubtitleStyle, CoreError>;

    fn create_export_job(
        &self,
        video_project_id: &str,
        mode: ExportMode,
        format: ExportFormat,
        quality: ExportQuality,
        background_hex: Option<&str>,
        include_audio: bool,
    ) -> Result<ExportJob, CoreError>;

    fn update_export_progress(
        &self,
        job_id: &str,
        progress_pct: i32,
        status: Option<ExportStatus>,
        encoder: Option<&str>,
    ) -> Result<(), CoreError>;

    fn complete_export_job(&self, job_id: &str, output_path: &str) -> Result<(), CoreError>;
    fn fail_export_job(&self, job_id: &str, error_message: &str) -> Result<(), CoreError>;
    fn get_export_job(&self, job_id: &str) -> Result<Option<ExportJob>, CoreError>;
}

/// Thread-safe SQLite repository backed by `rusqlite`.
#[derive(Clone)]
pub struct SqliteStudioRepository {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStudioRepository {
    /// Open (or create) the SQLite database at `path` and run schema migrations.
    pub fn open(path: &Path) -> Result<Self, CoreError> {
        let conn = Connection::open(path)?;
        let repo = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        repo.migrate()?;
        Ok(repo)
    }

    /// Open an in-memory SQLite database for tests.
    pub fn open_in_memory() -> Result<Self, CoreError> {
        let conn = Connection::open_in_memory()?;
        let repo = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        repo.migrate()?;
        Ok(repo)
    }

    fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, CoreError> {
        self.conn
            .lock()
            .map_err(|e| CoreError::InvalidInput(format!("database mutex poisoned: {e}")))
    }

    fn migrate(&self) -> Result<(), CoreError> {
        let conn = self.lock_conn()?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS video_projects (
                id TEXT PRIMARY KEY,
                original_filename TEXT NOT NULL,
                storage_path TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                width INTEGER NOT NULL,
                height INTEGER NOT NULL,
                fps REAL NOT NULL,
                status TEXT NOT NULL DEFAULT 'UPLOADED',
                language TEXT,
                error_message TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS transcript_words (
                id TEXT PRIMARY KEY,
                video_project_id TEXT NOT NULL REFERENCES video_projects(id) ON DELETE CASCADE,
                word TEXT NOT NULL,
                start_ms INTEGER NOT NULL,
                end_ms INTEGER NOT NULL,
                word_order INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_words_project_order
                ON transcript_words(video_project_id, word_order);

            CREATE TABLE IF NOT EXISTS subtitle_cues (
                id TEXT PRIMARY KEY,
                video_project_id TEXT NOT NULL REFERENCES video_projects(id) ON DELETE CASCADE,
                text TEXT NOT NULL,
                start_ms INTEGER NOT NULL,
                end_ms INTEGER NOT NULL,
                cue_order INTEGER NOT NULL,
                color_hex TEXT,
                edited INTEGER NOT NULL DEFAULT 0,
                word_start_idx INTEGER,
                word_end_idx INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_cues_project_order
                ON subtitle_cues(video_project_id, cue_order);

            CREATE TABLE IF NOT EXISTS subtitle_styles (
                id TEXT PRIMARY KEY,
                video_project_id TEXT NOT NULL UNIQUE REFERENCES video_projects(id) ON DELETE CASCADE,
                font_file_name TEXT NOT NULL,
                font_size_px REAL NOT NULL DEFAULT 42.0,
                font_color_hex TEXT NOT NULL DEFAULT '#FFFFFF',
                outline_color_hex TEXT NOT NULL DEFAULT '#000000',
                outline_width_px REAL NOT NULL DEFAULT 2.0,
                background_box_on INTEGER NOT NULL DEFAULT 0,
                background_color_hex TEXT NOT NULL DEFAULT '#000000',
                background_opacity REAL NOT NULL DEFAULT 0.5,
                position TEXT NOT NULL DEFAULT 'bottom',
                vertical_offset_pct INTEGER NOT NULL DEFAULT 10,
                position_x REAL NOT NULL DEFAULT 50.0,
                position_y REAL NOT NULL DEFAULT 80.0,
                rotation_deg REAL NOT NULL DEFAULT 0.0,
                animation TEXT NOT NULL DEFAULT 'pop',
                words_per_cue INTEGER NOT NULL DEFAULT 6,
                uppercase INTEGER NOT NULL DEFAULT 0,
                bold INTEGER NOT NULL DEFAULT 0,
                italic INTEGER NOT NULL DEFAULT 0,
                stroke_position TEXT NOT NULL DEFAULT 'outside',
                anim_in TEXT NOT NULL DEFAULT 'pop',
                anim_out TEXT NOT NULL DEFAULT 'none',
                anim_duration_ms INTEGER NOT NULL DEFAULT 300,
                easing TEXT NOT NULL DEFAULT 'cubic',
                karaoke_mode TEXT NOT NULL DEFAULT 'highlight-current-word',
                karaoke_color_hex TEXT NOT NULL DEFAULT '#FFE600',
                karaoke_scale REAL NOT NULL DEFAULT 1.1,
                shadow_json TEXT,
                glow_json TEXT,
                gradient_json TEXT,
                line_height REAL NOT NULL DEFAULT 1.25,
                letter_spacing REAL NOT NULL DEFAULT 0.0,
                box_radius REAL NOT NULL DEFAULT 8.0,
                box_padding_x REAL NOT NULL DEFAULT 12.0,
                box_padding_y REAL NOT NULL DEFAULT 6.0,
                max_width_pct REAL NOT NULL DEFAULT 88.0
            );

            CREATE TABLE IF NOT EXISTS export_jobs (
                id TEXT PRIMARY KEY,
                video_project_id TEXT NOT NULL REFERENCES video_projects(id) ON DELETE CASCADE,
                mode TEXT NOT NULL,
                background_hex TEXT,
                include_audio INTEGER NOT NULL DEFAULT 1,
                format TEXT NOT NULL,
                quality TEXT NOT NULL DEFAULT 'balanced',
                encoder TEXT,
                status TEXT NOT NULL DEFAULT 'QUEUED',
                progress_pct INTEGER NOT NULL DEFAULT 0,
                output_path TEXT,
                error_message TEXT,
                created_at TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    fn insert_default_style(conn: &Connection, project_id: &str) -> Result<SubtitleStyle, CoreError> {
        let style_id = format!("sty_{}", Uuid::new_v4().simple());
        let s = SubtitleStyle::default_for_project(style_id, project_id.to_string());
        Self::save_style_row(conn, &s)?;
        Ok(s)
    }

    fn save_style_row(conn: &Connection, s: &SubtitleStyle) -> Result<(), CoreError> {
        let shadow_json = s.shadow.as_ref().map(serde_json::to_string).transpose()?;
        let glow_json = s.glow.as_ref().map(serde_json::to_string).transpose()?;
        let gradient_json = s.gradient.as_ref().map(serde_json::to_string).transpose()?;
        let stroke_str = enum_to_kebab(&s.stroke_position)?;
        let anim_in_str = enum_to_kebab(&s.anim_in)?;
        let anim_out_str = enum_to_kebab(&s.anim_out)?;
        let easing_str = enum_to_kebab(&s.easing)?;
        let karaoke_str = enum_to_kebab(&s.karaoke_mode)?;

        conn.execute(
            "INSERT INTO subtitle_styles (
                id, video_project_id, font_file_name, font_size_px, font_color_hex,
                outline_color_hex, outline_width_px, background_box_on, background_color_hex,
                background_opacity, position, vertical_offset_pct, position_x, position_y,
                rotation_deg, animation, words_per_cue, uppercase, bold, italic,
                stroke_position, anim_in, anim_out, anim_duration_ms, easing,
                karaoke_mode, karaoke_color_hex, karaoke_scale, shadow_json, glow_json,
                gradient_json, line_height, letter_spacing, box_radius, box_padding_x,
                box_padding_y, max_width_pct
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8, ?9,
                ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19, ?20,
                ?21, ?22, ?23, ?24, ?25,
                ?26, ?27, ?28, ?29, ?30,
                ?31, ?32, ?33, ?34, ?35,
                ?36, ?37
            )
            ON CONFLICT(video_project_id) DO UPDATE SET
                font_file_name = excluded.font_file_name,
                font_size_px = excluded.font_size_px,
                font_color_hex = excluded.font_color_hex,
                outline_color_hex = excluded.outline_color_hex,
                outline_width_px = excluded.outline_width_px,
                background_box_on = excluded.background_box_on,
                background_color_hex = excluded.background_color_hex,
                background_opacity = excluded.background_opacity,
                position = excluded.position,
                vertical_offset_pct = excluded.vertical_offset_pct,
                position_x = excluded.position_x,
                position_y = excluded.position_y,
                rotation_deg = excluded.rotation_deg,
                animation = excluded.animation,
                words_per_cue = excluded.words_per_cue,
                uppercase = excluded.uppercase,
                bold = excluded.bold,
                italic = excluded.italic,
                stroke_position = excluded.stroke_position,
                anim_in = excluded.anim_in,
                anim_out = excluded.anim_out,
                anim_duration_ms = excluded.anim_duration_ms,
                easing = excluded.easing,
                karaoke_mode = excluded.karaoke_mode,
                karaoke_color_hex = excluded.karaoke_color_hex,
                karaoke_scale = excluded.karaoke_scale,
                shadow_json = excluded.shadow_json,
                glow_json = excluded.glow_json,
                gradient_json = excluded.gradient_json,
                line_height = excluded.line_height,
                letter_spacing = excluded.letter_spacing,
                box_radius = excluded.box_radius,
                box_padding_x = excluded.box_padding_x,
                box_padding_y = excluded.box_padding_y,
                max_width_pct = excluded.max_width_pct",
            params![
                s.id,
                s.video_project_id,
                s.font_file_name,
                s.font_size_px,
                s.font_color_hex,
                s.outline_color_hex,
                s.outline_width_px,
                i32::from(s.background_box_on),
                s.background_color_hex,
                s.background_opacity,
                s.position,
                s.vertical_offset_pct,
                s.position_x,
                s.position_y,
                s.rotation_deg,
                s.animation,
                s.words_per_cue,
                i32::from(s.uppercase),
                i32::from(s.bold),
                i32::from(s.italic),
                stroke_str,
                anim_in_str,
                anim_out_str,
                s.anim_duration_ms,
                easing_str,
                karaoke_str,
                s.karaoke_color_hex,
                s.karaoke_scale,
                shadow_json,
                glow_json,
                gradient_json,
                s.line_height,
                s.letter_spacing,
                s.box_radius,
                s.box_padding_x,
                s.box_padding_y,
                s.max_width_pct,
            ],
        )?;
        Ok(())
    }

    fn read_style(conn: &Connection, project_id: &str) -> Result<Option<SubtitleStyle>, CoreError> {
        conn.query_row(
            "SELECT
                id, video_project_id, font_file_name, font_size_px, font_color_hex,
                outline_color_hex, outline_width_px, background_box_on, background_color_hex,
                background_opacity, position, vertical_offset_pct, position_x, position_y,
                rotation_deg, animation, words_per_cue, uppercase, bold, italic,
                stroke_position, anim_in, anim_out, anim_duration_ms, easing,
                karaoke_mode, karaoke_color_hex, karaoke_scale, shadow_json, glow_json,
                gradient_json, line_height, letter_spacing, box_radius, box_padding_x,
                box_padding_y, max_width_pct
            FROM subtitle_styles WHERE video_project_id = ?1",
            params![project_id],
            |row| {
                let stroke_str: String = row.get(20)?;
                let anim_in_str: String = row.get(21)?;
                let anim_out_str: String = row.get(22)?;
                let easing_str: String = row.get(24)?;
                let karaoke_str: String = row.get(25)?;
                let shadow_json: Option<String> = row.get(28)?;
                let glow_json: Option<String> = row.get(29)?;
                let gradient_json: Option<String> = row.get(30)?;

                Ok(SubtitleStyle {
                    id: row.get(0)?,
                    video_project_id: row.get(1)?,
                    font_file_name: row.get(2)?,
                    font_size_px: row.get(3)?,
                    font_color_hex: row.get(4)?,
                    outline_color_hex: row.get(5)?,
                    outline_width_px: row.get(6)?,
                    background_box_on: row.get::<_, i32>(7)? != 0,
                    background_color_hex: row.get(8)?,
                    background_opacity: row.get(9)?,
                    position: row.get(10)?,
                    vertical_offset_pct: row.get(11)?,
                    position_x: row.get(12)?,
                    position_y: row.get(13)?,
                    rotation_deg: row.get(14)?,
                    animation: row.get(15)?,
                    words_per_cue: row.get(16)?,
                    uppercase: row.get::<_, i32>(17)? != 0,
                    bold: row.get::<_, i32>(18)? != 0,
                    italic: row.get::<_, i32>(19)? != 0,
                    stroke_position: parse_kebab(&stroke_str, StrokePosition::Outside),
                    anim_in: parse_kebab(&anim_in_str, AnimationIn::Pop),
                    anim_out: parse_kebab(&anim_out_str, AnimationOut::None),
                    anim_duration_ms: row.get(23)?,
                    easing: parse_kebab(&easing_str, EasingKind::Cubic),
                    karaoke_mode: parse_kebab(&karaoke_str, KaraokeMode::HighlightCurrentWord),
                    karaoke_color_hex: row.get(26)?,
                    karaoke_scale: row.get(27)?,
                    shadow: shadow_json.and_then(|s| serde_json::from_str(&s).ok()),
                    glow: glow_json.and_then(|s| serde_json::from_str(&s).ok()),
                    gradient: gradient_json.and_then(|s| serde_json::from_str(&s).ok()),
                    line_height: row.get(31)?,
                    letter_spacing: row.get(32)?,
                    box_radius: row.get(33)?,
                    box_padding_x: row.get(34)?,
                    box_padding_y: row.get(35)?,
                    max_width_pct: row.get(36)?,
                })
            },
        )
        .optional()
        .map_err(CoreError::from)
    }

    fn read_words(conn: &Connection, project_id: &str) -> Result<Vec<TranscriptWord>, CoreError> {
        let mut stmt = conn.prepare(
            "SELECT id, video_project_id, word, start_ms, end_ms, word_order
             FROM transcript_words WHERE video_project_id = ?1 ORDER BY word_order ASC",
        )?;
        let rows = stmt.query_map(params![project_id], |row| {
            Ok(TranscriptWord {
                id: row.get(0)?,
                video_project_id: row.get(1)?,
                word: row.get(2)?,
                start_ms: row.get(3)?,
                end_ms: row.get(4)?,
                order: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(CoreError::from)
    }

    fn read_cues(conn: &Connection, project_id: &str) -> Result<Vec<SubtitleCue>, CoreError> {
        let mut stmt = conn.prepare(
            "SELECT id, video_project_id, text, start_ms, end_ms, cue_order, color_hex, edited, word_start_idx, word_end_idx
             FROM subtitle_cues WHERE video_project_id = ?1 ORDER BY cue_order ASC",
        )?;
        let rows = stmt.query_map(params![project_id], |row| {
            let ws: Option<i64> = row.get(8)?;
            let we: Option<i64> = row.get(9)?;
            Ok(SubtitleCue {
                id: row.get(0)?,
                video_project_id: row.get(1)?,
                text: row.get(2)?,
                start_ms: row.get(3)?,
                end_ms: row.get(4)?,
                order: row.get(5)?,
                color_hex: row.get(6)?,
                edited: row.get::<_, i32>(7)? != 0,
                word_start_idx: ws.and_then(|v| usize::try_from(v).ok()),
                word_end_idx: we.and_then(|v| usize::try_from(v).ok()),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(CoreError::from)
    }

    fn read_export_jobs(conn: &Connection, project_id: &str) -> Result<Vec<ExportJob>, CoreError> {
        let mut stmt = conn.prepare(
            "SELECT id, video_project_id, mode, background_hex, include_audio, format, quality, encoder, status, progress_pct, output_path, error_message, created_at
             FROM export_jobs WHERE video_project_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![project_id], |row| {
            Ok(ExportJob {
                id: row.get(0)?,
                video_project_id: row.get(1)?,
                mode: ExportMode::from_db(&row.get::<_, String>(2)?),
                background_hex: row.get(3)?,
                include_audio: row.get::<_, i32>(4)? != 0,
                format: ExportFormat::from_db(&row.get::<_, String>(5)?),
                quality: ExportQuality::from_db(&row.get::<_, String>(6)?),
                encoder: row.get(7)?,
                status: ExportStatus::from_db(&row.get::<_, String>(8)?),
                progress_pct: row.get(9)?,
                output_path: row.get(10)?,
                error_message: row.get(11)?,
                created_at: row.get(12)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(CoreError::from)
    }
}

impl StudioRepository for SqliteStudioRepository {
    fn create_project(
        &self,
        original_filename: &str,
        storage_path: &str,
        metadata: VideoMetadata,
        language: Option<&str>,
    ) -> Result<VideoProject, CoreError> {
        let conn = self.lock_conn()?;
        let id = format!("vid_{}", Uuid::new_v4().simple());
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO video_projects (id, original_filename, storage_path, duration_ms, width, height, fps, status, language, error_message, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'UPLOADED', ?8, NULL, ?9, ?9)",
            params![
                id,
                original_filename,
                storage_path,
                metadata.duration_ms,
                i64::from(metadata.width),
                i64::from(metadata.height),
                metadata.fps,
                language,
                now,
            ],
        )?;
        let style = Self::insert_default_style(&conn, &id)?;
        Ok(VideoProject {
            id,
            original_filename: original_filename.to_string(),
            storage_path: storage_path.to_string(),
            duration_ms: metadata.duration_ms,
            width: metadata.width,
            height: metadata.height,
            fps: metadata.fps,
            status: PipelineStatus::Uploaded,
            language: language.map(ToOwned::to_owned),
            error_message: None,
            created_at: now.clone(),
            updated_at: now,
            words: Vec::new(),
            cues: Vec::new(),
            style: Some(style),
            export_jobs: Vec::new(),
            cue_count: 0,
        })
    }

    fn list_projects(&self) -> Result<Vec<VideoProject>, CoreError> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT p.id, p.original_filename, p.storage_path, p.duration_ms, p.width, p.height, p.fps,
                    p.status, p.language, p.error_message, p.created_at, p.updated_at,
                    (SELECT COUNT(*) FROM subtitle_cues c WHERE c.video_project_id = p.id) AS cue_count
             FROM video_projects p ORDER BY p.created_at DESC",
        )?;
        let base_rows: Vec<VideoProject> = stmt
            .query_map([], |row| {
                Ok(VideoProject {
                    id: row.get(0)?,
                    original_filename: row.get(1)?,
                    storage_path: row.get(2)?,
                    duration_ms: row.get(3)?,
                    width: row.get::<_, u32>(4)?,
                    height: row.get::<_, u32>(5)?,
                    fps: row.get(6)?,
                    status: PipelineStatus::from_db(&row.get::<_, String>(7)?),
                    language: row.get(8)?,
                    error_message: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                    words: Vec::new(),
                    cues: Vec::new(),
                    style: None,
                    export_jobs: Vec::new(),
                    cue_count: row.get::<_, i64>(12)?.max(0) as usize,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut out = Vec::with_capacity(base_rows.len());
        for mut proj in base_rows {
            proj.style = Self::read_style(&conn, &proj.id)?;
            out.push(proj);
        }
        Ok(out)
    }

    fn get_project(&self, id: &str) -> Result<Option<VideoProject>, CoreError> {
        let conn = self.lock_conn()?;
        let proj_opt = conn
            .query_row(
                "SELECT id, original_filename, storage_path, duration_ms, width, height, fps, status, language, error_message, created_at, updated_at
                 FROM video_projects WHERE id = ?1",
                params![id],
                |row| {
                    Ok(VideoProject {
                        id: row.get(0)?,
                        original_filename: row.get(1)?,
                        storage_path: row.get(2)?,
                        duration_ms: row.get(3)?,
                        width: row.get::<_, u32>(4)?,
                        height: row.get::<_, u32>(5)?,
                        fps: row.get(6)?,
                        status: PipelineStatus::from_db(&row.get::<_, String>(7)?),
                        language: row.get(8)?,
                        error_message: row.get(9)?,
                        created_at: row.get(10)?,
                        updated_at: row.get(11)?,
                        words: Vec::new(),
                        cues: Vec::new(),
                        style: None,
                        export_jobs: Vec::new(),
                        cue_count: 0,
                    })
                },
            )
            .optional()?;

        let Some(mut proj) = proj_opt else {
            return Ok(None);
        };
        proj.words = Self::read_words(&conn, id)?;
        proj.cues = Self::read_cues(&conn, id)?;
        proj.cue_count = proj.cues.len();
        proj.style = Self::read_style(&conn, id)?;
        proj.export_jobs = Self::read_export_jobs(&conn, id)?;
        Ok(Some(proj))
    }

    fn update_project_status(
        &self,
        id: &str,
        status: PipelineStatus,
        error_message: Option<&str>,
    ) -> Result<(), CoreError> {
        let conn = self.lock_conn()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE video_projects SET status = ?1, error_message = ?2, updated_at = ?3 WHERE id = ?4",
            params![status.as_str(), error_message, now, id],
        )?;
        Ok(())
    }

    fn update_project_language(&self, id: &str, language: Option<&str>) -> Result<(), CoreError> {
        let conn = self.lock_conn()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE video_projects SET language = ?1, updated_at = ?2 WHERE id = ?3",
            params![language, now, id],
        )?;
        Ok(())
    }

    fn delete_project(&self, id: &str) -> Result<(), CoreError> {
        let conn = self.lock_conn()?;
        let changed = conn.execute("DELETE FROM video_projects WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(CoreError::NotFound(format!("video project {id}")));
        }
        Ok(())
    }

    fn replace_words(
        &self,
        video_project_id: &str,
        words: &[(String, i64, i64)],
    ) -> Result<Vec<TranscriptWord>, CoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM transcript_words WHERE video_project_id = ?1",
            params![video_project_id],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO transcript_words (id, video_project_id, word, start_ms, end_ms, word_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for (idx, (word, start_ms, end_ms)) in words.iter().enumerate() {
                let wid = format!("w_{}", Uuid::new_v4().simple());
                stmt.execute(params![
                    wid,
                    video_project_id,
                    word,
                    start_ms,
                    end_ms,
                    idx as i32
                ])?;
            }
        }
        tx.commit()?;
        Self::read_words(&conn, video_project_id)
    }

    fn replace_cues(
        &self,
        video_project_id: &str,
        cues: &[CueInsertRow],
    ) -> Result<Vec<SubtitleCue>, CoreError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM subtitle_cues WHERE video_project_id = ?1",
            params![video_project_id],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO subtitle_cues (id, video_project_id, text, start_ms, end_ms, cue_order, color_hex, edited, word_start_idx, word_end_idx)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, 0, ?7, ?8)",
            )?;
            for (text, start_ms, end_ms, order, ws, we) in cues {
                let cid = format!("cue_{}", Uuid::new_v4().simple());
                stmt.execute(params![
                    cid,
                    video_project_id,
                    text,
                    start_ms,
                    end_ms,
                    order,
                    ws.map(|v| v as i64),
                    we.map(|v| v as i64),
                ])?;
            }
        }
        tx.commit()?;
        Self::read_cues(&conn, video_project_id)
    }

    fn create_cue(
        &self,
        video_project_id: &str,
        text: &str,
        start_ms: i64,
        end_ms: i64,
        color_hex: Option<&str>,
    ) -> Result<SubtitleCue, CoreError> {
        let conn = self.lock_conn()?;
        let order: i32 = conn.query_row(
            "SELECT COALESCE(MAX(cue_order) + 1, 0) FROM subtitle_cues WHERE video_project_id = ?1",
            params![video_project_id],
            |row| row.get(0),
        )?;
        let id = format!("cue_{}", Uuid::new_v4().simple());
        conn.execute(
            "INSERT INTO subtitle_cues (id, video_project_id, text, start_ms, end_ms, cue_order, color_hex, edited, word_start_idx, word_end_idx)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, NULL, NULL)",
            params![id, video_project_id, text, start_ms, end_ms, order, color_hex],
        )?;
        Ok(SubtitleCue {
            id,
            video_project_id: video_project_id.to_string(),
            text: text.to_string(),
            start_ms,
            end_ms,
            order,
            color_hex: color_hex.map(ToOwned::to_owned),
            edited: true,
            word_start_idx: None,
            word_end_idx: None,
        })
    }

    fn update_cue(
        &self,
        cue_id: &str,
        text: Option<&str>,
        start_ms: Option<i64>,
        end_ms: Option<i64>,
        color_hex: Option<Option<&str>>,
    ) -> Result<SubtitleCue, CoreError> {
        let conn = self.lock_conn()?;
        let existing = conn
            .query_row(
                "SELECT id, video_project_id, text, start_ms, end_ms, cue_order, color_hex, edited, word_start_idx, word_end_idx
                 FROM subtitle_cues WHERE id = ?1",
                params![cue_id],
                |row| {
                    let ws: Option<i64> = row.get(8)?;
                    let we: Option<i64> = row.get(9)?;
                    Ok(SubtitleCue {
                        id: row.get(0)?,
                        video_project_id: row.get(1)?,
                        text: row.get(2)?,
                        start_ms: row.get(3)?,
                        end_ms: row.get(4)?,
                        order: row.get(5)?,
                        color_hex: row.get(6)?,
                        edited: row.get::<_, i32>(7)? != 0,
                        word_start_idx: ws.and_then(|v| usize::try_from(v).ok()),
                        word_end_idx: we.and_then(|v| usize::try_from(v).ok()),
                    })
                },
            )
            .optional()?
            .ok_or_else(|| CoreError::NotFound(format!("cue {cue_id}")))?;

        let next_text = text.unwrap_or(&existing.text).to_string();
        let next_start = start_ms.unwrap_or(existing.start_ms);
        let next_end = end_ms.unwrap_or(existing.end_ms).max(next_start + 50);
        let next_color = match color_hex {
            Some(v) => v.map(ToOwned::to_owned),
            None => existing.color_hex.clone(),
        };
        // If cue text changed, clear word index range so renderer distributes timing proportionally.
        let (ws, we) = if text.is_some() && next_text != existing.text {
            (None, None)
        } else {
            (existing.word_start_idx, existing.word_end_idx)
        };

        conn.execute(
            "UPDATE subtitle_cues SET text = ?1, start_ms = ?2, end_ms = ?3, color_hex = ?4, edited = 1, word_start_idx = ?5, word_end_idx = ?6 WHERE id = ?7",
            params![
                next_text,
                next_start,
                next_end,
                next_color,
                ws.map(|v| v as i64),
                we.map(|v| v as i64),
                cue_id,
            ],
        )?;

        Ok(SubtitleCue {
            id: existing.id,
            video_project_id: existing.video_project_id,
            text: next_text,
            start_ms: next_start,
            end_ms: next_end,
            order: existing.order,
            color_hex: next_color,
            edited: true,
            word_start_idx: ws,
            word_end_idx: we,
        })
    }

    fn delete_cue(&self, cue_id: &str) -> Result<(), CoreError> {
        let conn = self.lock_conn()?;
        let changed = conn.execute("DELETE FROM subtitle_cues WHERE id = ?1", params![cue_id])?;
        if changed == 0 {
            return Err(CoreError::NotFound(format!("cue {cue_id}")));
        }
        Ok(())
    }

    fn update_style(
        &self,
        video_project_id: &str,
        patch: &UpdateStylePatch,
    ) -> Result<SubtitleStyle, CoreError> {
        let conn = self.lock_conn()?;
        let mut style = match Self::read_style(&conn, video_project_id)? {
            Some(s) => s,
            None => Self::insert_default_style(&conn, video_project_id)?,
        };

        if let Some(ref v) = patch.font_file_name {
            style.font_file_name = v.clone();
        }
        if let Some(v) = patch.font_size_px {
            style.font_size_px = v.clamp(8.0, 400.0);
        }
        if let Some(ref v) = patch.font_color_hex {
            style.font_color_hex = v.clone();
        }
        if let Some(ref v) = patch.outline_color_hex {
            style.outline_color_hex = v.clone();
        }
        if let Some(v) = patch.outline_width_px {
            style.outline_width_px = v.clamp(0.0, 24.0);
        }
        if let Some(v) = patch.background_box_on {
            style.background_box_on = v;
        }
        if let Some(ref v) = patch.background_color_hex {
            style.background_color_hex = v.clone();
        }
        if let Some(v) = patch.background_opacity {
            style.background_opacity = v.clamp(0.0, 1.0);
        }
        if let Some(ref v) = patch.position {
            style.position = v.clone();
        }
        if let Some(v) = patch.vertical_offset_pct {
            style.vertical_offset_pct = v.clamp(0, 100);
        }
        if let Some(v) = patch.position_x {
            style.position_x = v.clamp(0.0, 100.0);
        }
        if let Some(v) = patch.position_y {
            style.position_y = v.clamp(0.0, 100.0);
        }
        if let Some(v) = patch.rotation_deg {
            style.rotation_deg = v;
        }
        if let Some(ref v) = patch.animation {
            style.animation = v.clone();
        }
        if let Some(v) = patch.words_per_cue {
            style.words_per_cue = v.clamp(1, 20);
        }
        if let Some(v) = patch.uppercase {
            style.uppercase = v;
        }
        if let Some(v) = patch.bold {
            style.bold = v;
        }
        if let Some(v) = patch.italic {
            style.italic = v;
        }
        if let Some(v) = patch.stroke_position {
            style.stroke_position = v;
        }
        if let Some(v) = patch.anim_in {
            style.anim_in = v;
        }
        if let Some(v) = patch.anim_out {
            style.anim_out = v;
        }
        if let Some(v) = patch.anim_duration_ms {
            style.anim_duration_ms = v.clamp(50, 3000);
        }
        if let Some(v) = patch.easing {
            style.easing = v;
        }
        if let Some(v) = patch.karaoke_mode {
            style.karaoke_mode = v;
        }
        if let Some(ref v) = patch.karaoke_color_hex {
            style.karaoke_color_hex = v.clone();
        }
        if let Some(v) = patch.karaoke_scale {
            style.karaoke_scale = v.clamp(1.0, 2.0);
        }
        if let Some(ref v) = patch.shadow {
            style.shadow = v.clone();
        }
        if let Some(ref v) = patch.glow {
            style.glow = v.clone();
        }
        if let Some(ref v) = patch.gradient {
            style.gradient = v.clone();
        }
        if let Some(v) = patch.line_height {
            style.line_height = v.clamp(0.8, 2.5);
        }
        if let Some(v) = patch.letter_spacing {
            style.letter_spacing = v.clamp(-5.0, 30.0);
        }
        if let Some(v) = patch.box_radius {
            style.box_radius = v.clamp(0.0, 48.0);
        }
        if let Some(v) = patch.box_padding_x {
            style.box_padding_x = v.clamp(0.0, 64.0);
        }
        if let Some(v) = patch.box_padding_y {
            style.box_padding_y = v.clamp(0.0, 64.0);
        }
        if let Some(v) = patch.max_width_pct {
            style.max_width_pct = v.clamp(20.0, 100.0);
        }

        Self::save_style_row(&conn, &style)?;
        Ok(style)
    }

    fn create_export_job(
        &self,
        video_project_id: &str,
        mode: ExportMode,
        format: ExportFormat,
        quality: ExportQuality,
        background_hex: Option<&str>,
        include_audio: bool,
    ) -> Result<ExportJob, CoreError> {
        let conn = self.lock_conn()?;
        let id = format!("exp_{}", Uuid::new_v4().simple());
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO export_jobs (id, video_project_id, mode, background_hex, include_audio, format, quality, encoder, status, progress_pct, output_path, error_message, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, 'QUEUED', 0, NULL, NULL, ?8)",
            params![
                id,
                video_project_id,
                mode.as_str(),
                background_hex,
                i32::from(include_audio),
                format.as_str(),
                quality.as_str(),
                now,
            ],
        )?;
        Ok(ExportJob {
            id,
            video_project_id: video_project_id.to_string(),
            mode,
            background_hex: background_hex.map(ToOwned::to_owned),
            include_audio,
            format,
            quality,
            encoder: None,
            status: ExportStatus::Queued,
            progress_pct: 0,
            output_path: None,
            error_message: None,
            created_at: now,
        })
    }

    fn update_export_progress(
        &self,
        job_id: &str,
        progress_pct: i32,
        status: Option<ExportStatus>,
        encoder: Option<&str>,
    ) -> Result<(), CoreError> {
        let conn = self.lock_conn()?;
        if let Some(st) = status {
            if let Some(enc) = encoder {
                conn.execute(
                    "UPDATE export_jobs SET progress_pct = ?1, status = ?2, encoder = ?3 WHERE id = ?4",
                    params![progress_pct, st.as_str(), enc, job_id],
                )?;
            } else {
                conn.execute(
                    "UPDATE export_jobs SET progress_pct = ?1, status = ?2 WHERE id = ?3",
                    params![progress_pct, st.as_str(), job_id],
                )?;
            }
        } else {
            conn.execute(
                "UPDATE export_jobs SET progress_pct = ?1 WHERE id = ?2",
                params![progress_pct, job_id],
            )?;
        }
        Ok(())
    }

    fn complete_export_job(&self, job_id: &str, output_path: &str) -> Result<(), CoreError> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE export_jobs SET status = 'DONE', progress_pct = 100, output_path = ?1 WHERE id = ?2",
            params![output_path, job_id],
        )?;
        Ok(())
    }

    fn fail_export_job(&self, job_id: &str, error_message: &str) -> Result<(), CoreError> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE export_jobs SET status = 'FAILED', error_message = ?1 WHERE id = ?2",
            params![error_message, job_id],
        )?;
        Ok(())
    }

    fn get_export_job(&self, job_id: &str) -> Result<Option<ExportJob>, CoreError> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT id, video_project_id, mode, background_hex, include_audio, format, quality, encoder, status, progress_pct, output_path, error_message, created_at
             FROM export_jobs WHERE id = ?1",
            params![job_id],
            |row| {
                Ok(ExportJob {
                    id: row.get(0)?,
                    video_project_id: row.get(1)?,
                    mode: ExportMode::from_db(&row.get::<_, String>(2)?),
                    background_hex: row.get(3)?,
                    include_audio: row.get::<_, i32>(4)? != 0,
                    format: ExportFormat::from_db(&row.get::<_, String>(5)?),
                    quality: ExportQuality::from_db(&row.get::<_, String>(6)?),
                    encoder: row.get(7)?,
                    status: ExportStatus::from_db(&row.get::<_, String>(8)?),
                    progress_pct: row.get(9)?,
                    output_path: row.get(10)?,
                    error_message: row.get(11)?,
                    created_at: row.get(12)?,
                })
            },
        )
        .optional()
        .map_err(CoreError::from)
    }
}

fn enum_to_kebab<T: serde::Serialize>(val: &T) -> Result<String, CoreError> {
    let json = serde_json::to_value(val)?;
    Ok(json.as_str().unwrap_or_default().to_string())
}

fn parse_kebab<T: serde::de::DeserializeOwned>(s: &str, default: T) -> T {
    serde_json::from_value(serde_json::Value::String(s.to_string())).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_crud_and_cascade_work() {
        let repo = SqliteStudioRepository::open_in_memory().expect("open memory db");
        let proj = repo
            .create_project(
                "clip.mp4",
                "C:\\tmp\\clip.mp4",
                VideoMetadata {
                    duration_ms: 10_000,
                    width: 1920,
                    height: 1080,
                    fps: 30.0,
                },
                Some("hi"),
            )
            .expect("create project");

        assert_eq!(repo.list_projects().expect("list").len(), 1);

        let words = vec![("नमस्ते".to_string(), 100, 450)];
        let saved_words = repo.replace_words(&proj.id, &words).expect("words");
        assert_eq!(saved_words.len(), 1);

        let cue = repo
            .create_cue(&proj.id, "नमस्ते दुनिया", 100, 1200, Some("#FFE600"))
            .expect("create cue");
        assert!(cue.edited);

        let updated_style = repo
            .update_style(
                &proj.id,
                &UpdateStylePatch {
                    font_size_px: Some(54.0),
                    uppercase: Some(true),
                    ..Default::default()
                },
            )
            .expect("update style");
        assert_eq!(updated_style.font_size_px, 54.0);
        assert!(updated_style.uppercase);

        let detail = repo.get_project(&proj.id).expect("get").expect("exists");
        assert_eq!(detail.cues.len(), 1);
        assert_eq!(detail.words.len(), 1);

        repo.delete_project(&proj.id).expect("delete");
        assert!(repo.get_project(&proj.id).expect("get").is_none());
    }
}


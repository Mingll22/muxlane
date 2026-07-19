//! Durable, non-secret desktop workbench configuration.
//!
//! The module stores only explicit user configuration and submitted input
//! history. Terminal output, credentials, environment values, and Codex's
//! native session data never enter these tables.

use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use crate::{
    CoreError, CoreResult,
    layout::validate_id,
    model::{
        CommandPreset, CommandPresetTemplate, InputHistory, ProjectSettings, ProjectTemplate,
        TerminalPresetTemplate,
    },
    storage::{Storage, now},
};

const MAX_NAME: usize = 120;
const MAX_DESCRIPTION: usize = 500;
const MAX_COMMAND: usize = 8 * 1024;
const MAX_HISTORY: usize = 16 * 1024;
const MAX_HISTORY_RESULTS: u16 = 200;

pub fn project_settings(storage: &Storage, project_id: &str) -> CoreResult<ProjectSettings> {
    validate_id(project_id)?;
    storage.project(project_id)?;
    storage
        .connect()?
        .query_row(
            "SELECT project_id,runtime,default_account_id,default_model,reasoning,updated_at FROM project_settings WHERE project_id=?",
            [project_id],
            map_settings,
        )
        .optional()?
        .map_or_else(
            || {
                Ok(ProjectSettings {
                    project_id: project_id.to_owned(),
                    runtime: "codex".to_owned(),
                    default_account_id: None,
                    default_model: "gpt-5.6-sol".to_owned(),
                    reasoning: "high".to_owned(),
                    updated_at: 0,
                })
            },
            Ok,
        )
}

pub fn save_project_settings(
    storage: &Storage,
    project_id: &str,
    default_account_id: Option<&str>,
    default_model: &str,
    reasoning: &str,
) -> CoreResult<ProjectSettings> {
    validate_id(project_id)?;
    storage.project(project_id)?;
    if let Some(account_id) = default_account_id {
        validate_id(account_id)?;
        storage.account(account_id)?;
    }
    validate_model(default_model)?;
    validate_reasoning(reasoning)?;
    let timestamp = now();
    storage.connect()?.execute(
        "INSERT INTO project_settings(project_id,runtime,default_account_id,default_model,reasoning,updated_at) VALUES(?,'codex',?,?,?,?) ON CONFLICT(project_id) DO UPDATE SET default_account_id=excluded.default_account_id,default_model=excluded.default_model,reasoning=excluded.reasoning,updated_at=excluded.updated_at",
        params![project_id, default_account_id, default_model, reasoning, timestamp],
    )?;
    project_settings(storage, project_id)
}

pub fn list_templates(storage: &Storage) -> CoreResult<Vec<ProjectTemplate>> {
    let connection = storage.connect()?;
    let mut statement = connection.prepare(
        "SELECT template_id,name,description,default_model,reasoning,terminal_presets_json,command_presets_json,created_at,updated_at FROM project_templates ORDER BY name,template_id",
    )?;
    let rows = statement.query_map([], map_template)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
pub fn save_template(
    storage: &Storage,
    template_id: Option<&str>,
    name: &str,
    description: &str,
    default_model: &str,
    reasoning: &str,
    terminal_presets: Vec<TerminalPresetTemplate>,
    command_presets: Vec<CommandPresetTemplate>,
) -> CoreResult<ProjectTemplate> {
    validate_text(name, MAX_NAME, false, "template name")?;
    validate_text(description, MAX_DESCRIPTION, true, "template description")?;
    validate_model(default_model)?;
    validate_reasoning(reasoning)?;
    if terminal_presets.len() > 12 || command_presets.len() > 50 {
        return Err(CoreError::new("INVALID_REQUEST", "template preset limit exceeded"));
    }
    for preset in &terminal_presets {
        validate_text(&preset.name, MAX_NAME, false, "Terminal preset name")?;
        validate_terminal_kind(&preset.kind)?;
    }
    for preset in &command_presets {
        validate_command_template(preset)?;
    }
    let template_id = match template_id {
        Some(value) => {
            validate_id(value)?;
            value.to_owned()
        }
        None => format!("template_{}", Uuid::new_v4().simple()),
    };
    let timestamp = now();
    let terminal_json = serde_json::to_string(&terminal_presets)?;
    let command_json = serde_json::to_string(&command_presets)?;
    storage.connect()?.execute(
        "INSERT INTO project_templates(template_id,name,description,default_model,reasoning,terminal_presets_json,command_presets_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(template_id) DO UPDATE SET name=excluded.name,description=excluded.description,default_model=excluded.default_model,reasoning=excluded.reasoning,terminal_presets_json=excluded.terminal_presets_json,command_presets_json=excluded.command_presets_json,updated_at=excluded.updated_at",
        params![template_id, name.trim(), description.trim(), default_model, reasoning, terminal_json, command_json, timestamp, timestamp],
    )?;
    template(storage, &template_id)
}

pub fn copy_template(
    storage: &Storage,
    template_id: &str,
    name: &str,
) -> CoreResult<ProjectTemplate> {
    let source = template(storage, template_id)?;
    save_template(
        storage,
        None,
        name,
        &source.description,
        &source.default_model,
        &source.reasoning,
        source.terminal_presets,
        source.command_presets,
    )
}

pub fn delete_template(storage: &Storage, template_id: &str) -> CoreResult<()> {
    validate_id(template_id)?;
    let changed = storage
        .connect()?
        .execute("DELETE FROM project_templates WHERE template_id=?", [template_id])?;
    if changed == 1 {
        Ok(())
    } else {
        Err(CoreError::new("NOT_FOUND", "project template was not found"))
    }
}

pub fn apply_template(
    storage: &Storage,
    project_id: &str,
    template_id: &str,
) -> CoreResult<ProjectSettings> {
    let template = template(storage, template_id)?;
    let current = project_settings(storage, project_id)?;
    let settings = save_project_settings(
        storage,
        project_id,
        current.default_account_id.as_deref(),
        &template.default_model,
        &template.reasoning,
    )?;
    for preset in template.command_presets {
        let existing_id = command_preset_id_by_name(storage, project_id, &preset.name)?;
        let _ = save_command_preset(
            storage,
            existing_id.as_deref(),
            project_id,
            &preset.name,
            &preset.description,
            &preset.terminal_kind,
            &preset.working_directory,
            &preset.command,
        )?;
    }
    Ok(settings)
}

fn command_preset_id_by_name(
    storage: &Storage,
    project_id: &str,
    name: &str,
) -> CoreResult<Option<String>> {
    storage
        .connect()?
        .query_row(
            "SELECT preset_id FROM command_presets WHERE project_id=? AND name=?",
            params![project_id, name.trim()],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
}

pub fn list_command_presets(storage: &Storage, project_id: &str) -> CoreResult<Vec<CommandPreset>> {
    validate_id(project_id)?;
    storage.project(project_id)?;
    let connection = storage.connect()?;
    let mut statement = connection.prepare(
        "SELECT preset_id,project_id,name,description,terminal_kind,working_directory,command_text,created_at,updated_at FROM command_presets WHERE project_id=? ORDER BY name,preset_id",
    )?;
    let rows = statement.query_map([project_id], map_command_preset)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
pub fn save_command_preset(
    storage: &Storage,
    preset_id: Option<&str>,
    project_id: &str,
    name: &str,
    description: &str,
    terminal_kind: &str,
    working_directory: &str,
    command: &str,
) -> CoreResult<CommandPreset> {
    validate_id(project_id)?;
    storage.project(project_id)?;
    let template = CommandPresetTemplate {
        name: name.to_owned(),
        description: description.to_owned(),
        terminal_kind: terminal_kind.to_owned(),
        working_directory: working_directory.to_owned(),
        command: command.to_owned(),
    };
    validate_command_template(&template)?;
    let preset_id = match preset_id {
        Some(value) => {
            validate_id(value)?;
            value.to_owned()
        }
        None => format!("preset_{}", Uuid::new_v4().simple()),
    };
    let timestamp = now();
    storage.connect()?.execute(
        "INSERT INTO command_presets(preset_id,project_id,name,description,terminal_kind,working_directory,command_text,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(preset_id) DO UPDATE SET name=excluded.name,description=excluded.description,terminal_kind=excluded.terminal_kind,working_directory=excluded.working_directory,command_text=excluded.command_text,updated_at=excluded.updated_at",
        params![preset_id, project_id, name.trim(), description.trim(), terminal_kind, working_directory, command, timestamp, timestamp],
    )?;
    command_preset(storage, &preset_id)
}

pub fn delete_command_preset(storage: &Storage, preset_id: &str) -> CoreResult<()> {
    validate_id(preset_id)?;
    let changed =
        storage.connect()?.execute("DELETE FROM command_presets WHERE preset_id=?", [preset_id])?;
    if changed == 1 {
        Ok(())
    } else {
        Err(CoreError::new("NOT_FOUND", "command preset was not found"))
    }
}

pub fn append_history(
    storage: &Storage,
    project_id: &str,
    terminal_id: Option<&str>,
    thread_id: Option<&str>,
    kind: &str,
    input_text: &str,
) -> CoreResult<InputHistory> {
    validate_id(project_id)?;
    storage.project(project_id)?;
    if let Some(value) = terminal_id {
        validate_id(value)?;
        let terminal = storage.terminal(value)?;
        if terminal.project_id != project_id {
            return Err(CoreError::new("INVALID_REQUEST", "Terminal does not belong to Project"));
        }
    }
    if let Some(value) = thread_id {
        validate_id(value)?;
    }
    if !matches!(kind, "shell" | "prompt") {
        return Err(CoreError::new("INVALID_REQUEST", "history kind is invalid"));
    }
    validate_text(input_text, MAX_HISTORY, false, "history input")?;
    if looks_sensitive(input_text) {
        return Err(CoreError::new(
            "SENSITIVE_CONTENT_REJECTED",
            "input resembles a credential and was not recorded",
        ));
    }
    let entry = InputHistory {
        history_id: format!("history_{}", Uuid::new_v4().simple()),
        project_id: project_id.to_owned(),
        terminal_id: terminal_id.map(str::to_owned),
        thread_id: thread_id.map(str::to_owned),
        kind: kind.to_owned(),
        input_text: input_text.to_owned(),
        created_at: now(),
    };
    storage.connect()?.execute(
        "INSERT INTO input_history(history_id,project_id,terminal_id,thread_id,kind,input_text,created_at) VALUES(?,?,?,?,?,?,?)",
        params![entry.history_id, entry.project_id, entry.terminal_id, entry.thread_id, entry.kind, entry.input_text, entry.created_at],
    )?;
    Ok(entry)
}

#[allow(clippy::too_many_arguments)]
pub fn search_history(
    storage: &Storage,
    project_id: &str,
    terminal_id: Option<&str>,
    thread_id: Option<&str>,
    kind: Option<&str>,
    query: &str,
    limit: u16,
) -> CoreResult<Vec<InputHistory>> {
    validate_id(project_id)?;
    let limit = limit.clamp(1, MAX_HISTORY_RESULTS);
    let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
    let connection = storage.connect()?;
    let mut statement = connection.prepare(
        "SELECT history_id,project_id,terminal_id,thread_id,kind,input_text,created_at FROM input_history WHERE project_id=? AND (?2 IS NULL OR terminal_id=?2) AND (?3 IS NULL OR thread_id=?3) AND (?4 IS NULL OR kind=?4) AND input_text LIKE ?5 ESCAPE '\\' ORDER BY created_at DESC,history_id LIMIT ?6",
    )?;
    let rows = statement.query_map(
        params![project_id, terminal_id, thread_id, kind, pattern, limit],
        map_history,
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn delete_history(storage: &Storage, history_id: &str) -> CoreResult<()> {
    validate_id(history_id)?;
    let changed =
        storage.connect()?.execute("DELETE FROM input_history WHERE history_id=?", [history_id])?;
    if changed == 1 {
        Ok(())
    } else {
        Err(CoreError::new("NOT_FOUND", "history entry was not found"))
    }
}

pub fn clear_project_history(storage: &Storage, project_id: &str) -> CoreResult<u64> {
    validate_id(project_id)?;
    storage.project(project_id)?;
    let changed =
        storage.connect()?.execute("DELETE FROM input_history WHERE project_id=?", [project_id])?;
    Ok(changed as u64)
}

fn template(storage: &Storage, template_id: &str) -> CoreResult<ProjectTemplate> {
    validate_id(template_id)?;
    storage
        .connect()?
        .query_row(
            "SELECT template_id,name,description,default_model,reasoning,terminal_presets_json,command_presets_json,created_at,updated_at FROM project_templates WHERE template_id=?",
            [template_id],
            map_template,
        )
        .optional()?
        .ok_or_else(|| CoreError::new("NOT_FOUND", "project template was not found"))
}

fn command_preset(storage: &Storage, preset_id: &str) -> CoreResult<CommandPreset> {
    storage
        .connect()?
        .query_row(
            "SELECT preset_id,project_id,name,description,terminal_kind,working_directory,command_text,created_at,updated_at FROM command_presets WHERE preset_id=?",
            [preset_id],
            map_command_preset,
        )
        .optional()?
        .ok_or_else(|| CoreError::new("NOT_FOUND", "command preset was not found"))
}

fn map_settings(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectSettings> {
    Ok(ProjectSettings {
        project_id: row.get(0)?,
        runtime: row.get(1)?,
        default_account_id: row.get(2)?,
        default_model: row.get(3)?,
        reasoning: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn map_template(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectTemplate> {
    let terminal_json: String = row.get(5)?;
    let command_json: String = row.get(6)?;
    Ok(ProjectTemplate {
        template_id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        default_model: row.get(3)?,
        reasoning: row.get(4)?,
        terminal_presets: serde_json::from_str(&terminal_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                terminal_json.len(),
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        command_presets: serde_json::from_str(&command_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                command_json.len(),
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn map_command_preset(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommandPreset> {
    Ok(CommandPreset {
        preset_id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        terminal_kind: row.get(4)?,
        working_directory: row.get(5)?,
        command: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn map_history(row: &rusqlite::Row<'_>) -> rusqlite::Result<InputHistory> {
    Ok(InputHistory {
        history_id: row.get(0)?,
        project_id: row.get(1)?,
        terminal_id: row.get(2)?,
        thread_id: row.get(3)?,
        kind: row.get(4)?,
        input_text: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn validate_model(value: &str) -> CoreResult<()> {
    if value.is_empty()
        || value.len() > 80
        || !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(CoreError::new("INVALID_REQUEST", "model identifier is invalid"));
    }
    Ok(())
}

fn validate_reasoning(value: &str) -> CoreResult<()> {
    if matches!(value, "low" | "medium" | "high" | "xhigh") {
        Ok(())
    } else {
        Err(CoreError::new("INVALID_REQUEST", "reasoning value is invalid"))
    }
}

fn validate_terminal_kind(value: &str) -> CoreResult<()> {
    if matches!(value, "codex" | "shell" | "auxiliary") {
        Ok(())
    } else {
        Err(CoreError::new("INVALID_REQUEST", "Terminal kind is invalid"))
    }
}

fn validate_command_template(value: &CommandPresetTemplate) -> CoreResult<()> {
    validate_text(&value.name, MAX_NAME, false, "command preset name")?;
    validate_text(&value.description, MAX_DESCRIPTION, true, "command preset description")?;
    validate_terminal_kind(&value.terminal_kind)?;
    validate_relative_path(&value.working_directory)?;
    validate_text(&value.command, MAX_COMMAND, false, "command preset")
}

fn validate_relative_path(value: &str) -> CoreResult<()> {
    if value.is_empty() {
        return Ok(());
    }
    let path = std::path::Path::new(value);
    if path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(CoreError::new("PATH_REJECTED", "working directory must stay in Project"));
    }
    Ok(())
}

fn validate_text(value: &str, max: usize, allow_empty: bool, label: &str) -> CoreResult<()> {
    if (!allow_empty && value.trim().is_empty())
        || value.len() > max
        || value.contains('\0')
        || value
            .chars()
            .any(|character| character.is_control() && character != '\n' && character != '\t')
    {
        return Err(CoreError::new("INVALID_REQUEST", format!("{label} is invalid")));
    }
    Ok(())
}

fn looks_sensitive(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let markers = [
        "authorization: bearer ",
        "access_token",
        "refresh_token",
        "client_secret",
        "api_key",
        "private key-----",
    ];
    markers.iter().any(|marker| lower.contains(marker))
        || value.split_whitespace().any(|part| {
            part.len() >= 80
                && part.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::{layout::Layout, service};

    #[test]
    fn persists_templates_presets_and_history_without_terminal_output() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        let storage =
            Storage::open(Layout::initialize(temp.path().join("runtime")).unwrap()).unwrap();
        let project = service::register_project(&storage, &source, "demo").unwrap();
        let template = save_template(
            &storage,
            None,
            "Rust",
            "Rust defaults",
            "gpt-5.6-sol",
            "high",
            vec![TerminalPresetTemplate { name: "Shell".into(), kind: "shell".into() }],
            vec![CommandPresetTemplate {
                name: "Test".into(),
                description: "Run tests".into(),
                terminal_kind: "shell".into(),
                working_directory: "".into(),
                command: "cargo test".into(),
            }],
        )
        .unwrap();
        apply_template(&storage, &project.project_id, &template.template_id).unwrap();
        apply_template(&storage, &project.project_id, &template.template_id).unwrap();
        assert_eq!(list_command_presets(&storage, &project.project_id).unwrap().len(), 1);
        append_history(&storage, &project.project_id, None, None, "shell", "cargo test").unwrap();
        assert_eq!(
            search_history(&storage, &project.project_id, None, None, None, "cargo", 10)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            append_history(
                &storage,
                &project.project_id,
                None,
                None,
                "shell",
                "Authorization: Bearer secret"
            )
            .unwrap_err()
            .code,
            "SENSITIVE_CONTENT_REJECTED"
        );
    }
}

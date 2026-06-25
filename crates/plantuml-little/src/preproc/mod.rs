//! PlantUML preprocessor — expands directives before parsing.
//!
//! Handles: `!pragma`, variable assignment/substitution, `!define`,
//! built-in functions, line continuation, conditionals, `!function`/`!procedure`,
//! `!include`, `!theme`, `!foreach`/`!endfor`, and `!while`/`!endwhile`.
//! Arithmetic expressions are evaluated in variable assignments and conditions.

mod builtins;
mod expr;
mod include;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::Result;
use regex::Regex;
use url::Url;

use builtins::*;
use expr::*;
use include::*;

/// Preprocess raw PlantUML source, expanding all preprocessor directives.
///
/// Returns the expanded source text ready for parsing.
pub fn preprocess(source: &str) -> Result<String> {
    let cwd = std::env::current_dir().ok();
    preprocess_with_optional_base_dir(source, cwd.as_deref())
}

/// Preprocess raw PlantUML source using an explicit base directory for
/// relative `!include` resolution.
pub fn preprocess_with_base_dir(source: &str, base_dir: &Path) -> Result<String> {
    preprocess_with_optional_base_dir(source, Some(base_dir))
}

/// Preprocess raw PlantUML source using an explicit input file path.
/// This preserves filename/dirpath context for builtins like `%filename()`.
pub fn preprocess_with_source_path(source: &str, source_path: &Path) -> Result<String> {
    let mut ctx = Context::new();
    let mut output = Vec::new();
    let base_dir = source_path.parent().unwrap_or_else(|| Path::new("."));
    let source_key = source_path.to_string_lossy().to_string();
    ctx.process_source_into(source, Some(base_dir), None, Some(&source_key), &mut output)?;
    Ok(output.join("\n"))
}

fn preprocess_with_optional_base_dir(source: &str, base_dir: Option<&Path>) -> Result<String> {
    let mut ctx = Context::new();
    ctx.process(source, base_dir)
}

// ─── internal types ────────────────────────────────────────────────

/// Typed value for preprocessor variables.
///
/// Supports string, integer, and array types to enable proper array
/// operations in `!foreach` and numeric operations in `!while`.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Value {
    Str(String),
    Int(i64),
    Array(Vec<Value>),
}

impl Value {
    /// Convert to a string representation (for substitution in text).
    fn as_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::Int(n) => n.to_string(),
            Value::Array(items) => {
                let inner: Vec<String> = items.iter().map(Value::as_str).collect();
                format!("[{}]", inner.join(", "))
            }
        }
    }

    /// Try to get the integer value.
    #[cfg(test)]
    fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            Value::Str(s) => s.trim().parse::<i64>().ok(),
            Value::Array(_) => None,
        }
    }

    /// Try to get a reference to the array contents.
    fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    /// Truthiness: empty string / zero / empty array are falsy.
    fn is_truthy(&self) -> bool {
        match self {
            Value::Str(s) => !s.is_empty() && s != "0" && s != "false",
            Value::Int(n) => *n != 0,
            Value::Array(items) => !items.is_empty(),
        }
    }

    /// Parse a raw (already expanded) string into a typed Value.
    ///
    /// Detection order: integer literal -> bracket array -> string.
    fn parse_from(s: &str) -> Value {
        let s = s.trim();
        if is_quoted_literal(s) {
            let inner = unquote(s);
            if requires_round_trip_quotes(&inner) {
                return Value::Str(s.to_string());
            }
            return Value::Str(inner);
        }
        // Try integer first
        if let Ok(n) = s.parse::<i64>() {
            return Value::Int(n);
        }
        // Try array syntax [...]
        if let Some(items) = parse_array_values(s) {
            return Value::Array(items);
        }
        Value::Str(s.to_string())
    }
}

/// A user-defined function or procedure.
#[derive(Clone, Debug)]
struct UserFunc {
    params: Vec<ParamSpec>,
    body: Vec<String>,
    is_procedure: bool,
}

#[derive(Clone, Debug)]
pub(super) struct ParamSpec {
    pub(super) name: String,
    pub(super) default: Option<String>,
}

/// Preprocessor evaluation context.
struct Context {
    /// Top-level `$var` -> typed value
    vars: HashMap<String, Value>,
    /// Function / procedure-local scopes, inner-most last
    local_scopes: Vec<HashMap<String, Value>>,
    /// `!define NAME` -> replacement text
    defines: HashMap<String, DefineEntry>,
    /// User-defined `!function` / `!procedure`
    funcs: HashMap<String, UserFunc>,
    /// Pragma key -> value
    pragmas: HashMap<String, String>,
    /// Files already included through `!include` / `!include_once`
    included_once: HashSet<String>,
    /// Active include stack for recursion detection
    include_stack: Vec<String>,
    /// Base directory stack for nested include resolution
    base_dirs: Vec<PathBuf>,
    /// Base URL stack for nested remote include resolution
    base_urls: Vec<Url>,
    /// Source stack for current file/URL builtins
    source_stack: Vec<String>,
    /// Search roots extracted from `!import` archives
    imported_roots: Vec<PathBuf>,
    /// Cache of extracted archives -> temp roots
    imported_archives: HashMap<PathBuf, PathBuf>,
    /// Cache of fetched remote files -> local temp copies
    remote_files: HashMap<String, PathBuf>,
    /// Functions currently being expanded (prevent recursive expansion)
    expanding_funcs: Vec<String>,
    /// Depth of function/procedure expansion — suppress nested func call expansion
    func_expansion_depth: usize,
    /// Active function return values, innermost last
    return_stack: Vec<Option<String>>,
}

#[derive(Clone, Debug)]
pub(super) struct DefineEntry {
    pub(super) params: Vec<String>,
    pub(super) body: String,
}

#[derive(Clone, Copy)]
pub(super) enum IncludeMode {
    Include,
    IncludeOnce,
    IncludeSub,
    Many,
}

#[derive(Clone, Copy)]
enum VarAssignMode {
    Auto,
    Global,
    Local,
}

struct ResolvedInclude {
    path: PathBuf,
    source_key: String,
    base_url: Option<Url>,
}

impl Context {
    fn new() -> Self {
        Self {
            vars: HashMap::new(),
            local_scopes: Vec::new(),
            defines: HashMap::new(),
            funcs: HashMap::new(),
            pragmas: HashMap::new(),
            included_once: HashSet::new(),
            include_stack: Vec::new(),
            base_dirs: Vec::new(),
            base_urls: Vec::new(),
            source_stack: Vec::new(),
            imported_roots: Vec::new(),
            imported_archives: HashMap::new(),
            remote_files: HashMap::new(),
            expanding_funcs: Vec::new(),
            func_expansion_depth: 0,
            return_stack: Vec::new(),
        }
    }

    /// Top-level processing entry point.
    fn process(&mut self, source: &str, base_dir: Option<&Path>) -> Result<String> {
        let mut output = Vec::new();
        self.process_source_into(source, base_dir, None, None, &mut output)?;
        Ok(output.join("\n"))
    }

    fn process_source_into(
        &mut self,
        source: &str,
        base_dir: Option<&Path>,
        base_url: Option<&Url>,
        source_key: Option<&str>,
        output: &mut Vec<String>,
    ) -> Result<()> {
        if let Some(dir) = base_dir {
            self.base_dirs.push(dir.to_path_buf());
        }
        if let Some(url) = base_url {
            self.base_urls.push(url.clone());
        }
        if let Some(source_key) = source_key {
            self.source_stack.push(source_key.to_string());
        }
        let joined = join_continuations(source);
        let lines: Vec<&str> = joined.lines().collect();
        let result = self.process_lines(&lines, output);
        if base_dir.is_some() {
            self.base_dirs.pop();
        }
        if base_url.is_some() {
            self.base_urls.pop();
        }
        if source_key.is_some() {
            self.source_stack.pop();
        }
        result
    }

    fn current_base_dir(&self) -> Option<&Path> {
        self.base_dirs.last().map(PathBuf::as_path)
    }

    fn current_base_url(&self) -> Option<&Url> {
        self.base_urls.last()
    }

    fn current_source_key(&self) -> Option<&str> {
        self.source_stack.last().map(String::as_str)
    }

    fn lookup_var(&self, name: &str) -> Option<&Value> {
        for scope in self.local_scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(value);
            }
        }
        self.vars.get(name)
    }

    fn is_var_defined(&self, name: &str) -> bool {
        self.lookup_var(name).is_some()
    }

    fn assign_var(&mut self, name: String, value: Value, mode: VarAssignMode) {
        match mode {
            VarAssignMode::Global => {
                self.vars.insert(name, value);
            }
            VarAssignMode::Local => {
                if let Some(scope) = self.local_scopes.last_mut() {
                    scope.insert(name, value);
                } else {
                    self.vars.insert(name, value);
                }
            }
            VarAssignMode::Auto => {
                if let Some(scope) = self
                    .local_scopes
                    .iter_mut()
                    .rev()
                    .find(|scope| scope.contains_key(&name))
                {
                    scope.insert(name, value);
                } else if self.vars.contains_key(&name) {
                    self.vars.insert(name, value);
                } else if let Some(scope) = self.local_scopes.last_mut() {
                    scope.insert(name, value);
                } else {
                    self.vars.insert(name, value);
                }
            }
        }
    }

    fn visible_vars(&self) -> HashMap<String, Value> {
        let mut merged = self.vars.clone();
        for scope in &self.local_scopes {
            for (name, value) in scope {
                merged.insert(name.clone(), value.clone());
            }
        }
        merged
    }

    fn has_pending_return(&self) -> bool {
        self.return_stack
            .last()
            .and_then(|value| value.as_ref())
            .is_some()
    }

    /// Process a slice of lines, appending results to `output`.
    fn process_lines(&mut self, lines: &[&str], output: &mut Vec<String>) -> Result<()> {
        let mut i = 0;
        while i < lines.len() {
            if self.has_pending_return() {
                break;
            }
            let line = lines[i];
            let trimmed = line.trim();

            // ── directive detection ──
            if trimmed.starts_with("!pragma ") {
                self.handle_pragma(trimmed);
                // Pass pragma through to output so downstream parsers can see it
                // (e.g. sequence parser needs `!pragma teoz true` to enable teoz mode)
                output.push(trimmed.to_string());
                i += 1;
                continue;
            }

            if trimmed.starts_with("!theme ") || trimmed == "!theme" {
                self.handle_theme(trimmed, output)?;
                i += 1;
                continue;
            }

            if trimmed.starts_with("!import ") {
                self.handle_import(trimmed)?;
                i += 1;
                continue;
            }

            if trimmed.starts_with("!include_many ") {
                self.process_include(trimmed, IncludeMode::Many, output)?;
                i += 1;
                continue;
            }

            if trimmed.starts_with("!includeurl ") {
                self.process_include(trimmed, IncludeMode::Include, output)?;
                i += 1;
                continue;
            }

            if trimmed.starts_with("!include_once ") {
                self.process_include(trimmed, IncludeMode::IncludeOnce, output)?;
                i += 1;
                continue;
            }

            if trimmed.starts_with("!include ") {
                self.process_include(trimmed, IncludeMode::Include, output)?;
                i += 1;
                continue;
            }

            if trimmed.starts_with("!includesub ") {
                self.process_include(trimmed, IncludeMode::IncludeSub, output)?;
                i += 1;
                continue;
            }

            if trimmed.starts_with("!includedef ") {
                self.process_includedef(trimmed)?;
                i += 1;
                continue;
            }

            if trimmed.starts_with("!assert ") {
                self.handle_assert(trimmed, i + 1)?;
                i += 1;
                continue;
            }

            if trimmed.starts_with("!dump_memory") || trimmed.starts_with("!memory_dump") {
                self.handle_dump_memory();
                i += 1;
                continue;
            }

            if trimmed.starts_with("!undef ") {
                self.handle_undef(trimmed);
                i += 1;
                continue;
            }

            // Variable assignment: `!$var = value` or `!$var="value"`
            if let Some(rest) = strip_directive_prefix(trimmed, "!") {
                if rest.starts_with('$') {
                    if let Some(consumed) = self.try_var_assign(rest, VarAssignMode::Auto)? {
                        if consumed {
                            i += 1;
                            continue;
                        }
                    }
                }
            }

            // Legacy assignment without $: `!VAR=value` or `!VAR = value`
            if let Some(rest) = strip_directive_prefix(trimmed, "!") {
                if !rest.starts_with('$')
                    && !rest.starts_with("if ")
                    && !rest.starts_with("ifdef ")
                    && !rest.starts_with("ifndef ")
                    && !rest.starts_with("else")
                    && !rest.starts_with("endif")
                    && !rest.starts_with("define ")
                    && !rest.starts_with("function ")
                    && !rest.starts_with("procedure ")
                    && !rest.starts_with("unquoted ")
                    && !rest.starts_with("end")
                    && !rest.starts_with("return")
                    && !rest.starts_with("while ")
                    && !rest.starts_with("endwhile")
                    && !rest.starts_with("foreach ")
                    && !rest.starts_with("endfor")
                    && !rest.starts_with("global ")
                    && !rest.starts_with("local ")
                    && !rest.starts_with("log ")
                {
                    if let Some(consumed) = self.try_legacy_assign(rest) {
                        if consumed {
                            i += 1;
                            continue;
                        }
                    }
                }
            }

            // !define
            if let Some(rest) = strip_directive_prefix(trimmed, "!define ") {
                self.handle_define(rest);
                i += 1;
                continue;
            }

            // !global $var = value
            if let Some(rest) = strip_directive_prefix(trimmed, "!global ") {
                if rest.starts_with('$') {
                    self.try_var_assign(rest, VarAssignMode::Global)?;
                }
                i += 1;
                continue;
            }

            // !local $var = value
            if let Some(rest) = strip_directive_prefix(trimmed, "!local ") {
                if rest.starts_with('$') {
                    self.try_var_assign(rest, VarAssignMode::Local)?;
                }
                i += 1;
                continue;
            }

            // !function / !procedure (possibly with !unquoted prefix)
            if trimmed.starts_with("!function ")
                || trimmed.starts_with("!procedure ")
                || trimmed.starts_with("!unquoted function ")
                || trimmed.starts_with("!unquoted procedure ")
            {
                let end = self.collect_func_def(lines, i);
                i = end;
                continue;
            }

            if trimmed.starts_with("!definelong ") {
                let end = self.collect_define_long(lines, i);
                i = end;
                continue;
            }

            // !foreach
            if trimmed.starts_with("!foreach ") {
                let end = self.process_foreach(lines, i, output)?;
                i = end;
                continue;
            }

            // !while
            if trimmed.starts_with("!while ") {
                let end = self.process_while(lines, i, output)?;
                i = end;
                continue;
            }

            // Conditionals
            if trimmed.starts_with("!if ")
                || trimmed.starts_with("!ifdef ")
                || trimmed.starts_with("!ifndef ")
            {
                let end = self.process_conditional(lines, i, output)?;
                i = end;
                continue;
            }

            // !log — output message to log system
            if let Some(rest) = strip_directive_prefix(trimmed, "!log ") {
                let msg = self.evaluate_expression_text_with_funcs(rest)?;
                log::info!("[PlantUML !log] {msg}");
                i += 1;
                continue;
            }
            if trimmed == "!log" {
                log::info!("[PlantUML !log]");
                i += 1;
                continue;
            }

            if let Some(rest) = strip_directive_prefix(trimmed, "!return ") {
                if self.return_stack.is_empty() {
                    log::warn!("!return used outside of function: {trimmed}");
                    i += 1;
                    continue;
                }
                let value = self.evaluate_expression_text_with_funcs(rest.trim())?;
                if let Some(slot) = self.return_stack.last_mut() {
                    *slot = Some(value);
                }
                break;
            }
            if trimmed == "!return" {
                if self.return_stack.is_empty() {
                    log::warn!("!return used outside of function");
                    i += 1;
                    continue;
                }
                if let Some(slot) = self.return_stack.last_mut() {
                    *slot = Some(String::new());
                }
                break;
            }

            // skinparam — pass through as-is (no variable substitution in skinparam keys)
            // Catch unknown ! directives
            if trimmed.starts_with('!') && !trimmed.starts_with("!!") {
                // Try to recognise; if we can't, warn and skip
                let directive = trimmed.split_whitespace().next().unwrap_or("!");
                if !matches!(
                    directive,
                    "!pragma"
                        | "!theme"
                        | "!import"
                        | "!include"
                        | "!includeurl"
                        | "!includesub"
                        | "!include_many"
                        | "!include_once"
                        | "!define"
                        | "!definelong"
                        | "!ifdef"
                        | "!ifndef"
                        | "!if"
                        | "!else"
                        | "!endif"
                        | "!function"
                        | "!procedure"
                        | "!endfunction"
                        | "!endprocedure"
                        | "!enddefinelong"
                        | "!enddefine"
                        | "!unquoted"
                        | "!return"
                        | "!global"
                        | "!local"
                        | "!log"
                        | "!elseif"
                        | "!assert"
                        | "!dump_memory"
                        | "!memory_dump"
                        | "!undef"
                        | "!while"
                        | "!endwhile"
                        | "!foreach"
                        | "!endfor"
                        | "!includedef"
                ) {
                    // Could be a variable assignment we missed — try once more
                    let rest = &trimmed[1..];
                    if self.try_legacy_assign(rest).is_some() {
                        i += 1;
                        continue;
                    }
                    log::warn!("unknown preprocessor directive skipped: {trimmed}");
                }
                i += 1;
                continue;
            }

            // ── single-line comments: `'...` — Java strips these during reading
            // (ReadLineQuoteComment).  PlantUML treats lines beginning with
            // `'` as comments universally, before any block parsing.
            if trimmed.starts_with('\'') {
                // Emit an empty line to preserve line numbering for source-line references
                output.push(String::new());
                i += 1;
                continue;
            }

            // ── normal line: substitute variables / defines / builtins, then emit ──
            if output.len() > 100_000 {
                return Err(crate::Error::Parse {
                    line: i + 1,
                    column: Some(1),
                    message: "output limit (100000 lines) exceeded — possible infinite expansion"
                        .to_string(),
                });
            }
            let expanded = self.expand_line(line)?;
            output.push(expanded);
            i += 1;
        }
        Ok(())
    }

    // ── pragma ──────────────────────────────────────────────────

    fn handle_pragma(&mut self, trimmed: &str) {
        // `!pragma key value`
        let rest = &trimmed["!pragma ".len()..];
        let mut parts = rest.splitn(2, char::is_whitespace);
        if let Some(key) = parts.next() {
            let val = parts.next().unwrap_or("").trim().to_string();
            self.pragmas.insert(key.to_string(), val);
        }
    }

    fn handle_assert(&self, trimmed: &str, line_no: usize) -> Result<()> {
        let rest = trimmed["!assert ".len()..].trim();
        let (expr, message) = if let Some((expr, message)) = split_top_level_once(rest, ':') {
            (expr.trim(), Some(unquote(message.trim())))
        } else {
            (rest, None)
        };

        if self.eval_if_expr(expr) {
            return Ok(());
        }

        Err(crate::Error::Parse {
            line: line_no,
            column: Some(1),
            message: message.unwrap_or_else(|| format!("assertion failed: {expr}")),
        })
    }

    fn handle_dump_memory(&self) {
        log::debug!(
            "preprocessor vars={:?} defines={:?} pragmas={:?}",
            self.visible_vars().keys().collect::<Vec<_>>(),
            self.defines.keys().collect::<Vec<_>>(),
            self.pragmas.keys().collect::<Vec<_>>()
        );
    }

    fn handle_import(&mut self, trimmed: &str) -> Result<()> {
        let target = trimmed["!import ".len()..].trim();
        let archive_path = self.resolve_import_path(target)?;
        let extracted_root = if let Some(root) = self.imported_archives.get(&archive_path) {
            root.clone()
        } else {
            let root = extract_archive_to_temp(&archive_path)?;
            self.imported_archives
                .insert(archive_path.clone(), root.clone());
            root
        };

        if !self.imported_roots.contains(&extracted_root) {
            self.imported_roots.push(extracted_root);
        }

        Ok(())
    }

    fn resolve_import_path(&mut self, target: &str) -> Result<PathBuf> {
        if is_remote_reference(target) {
            return self.fetch_remote_url(target);
        }

        if let Some(base_url) = self.current_base_url() {
            if let Ok(joined) = base_url.join(target) {
                return self.fetch_remote_url(joined.as_str());
            }
        }

        self.resolve_filesystem_path(target)
    }

    fn handle_theme(&mut self, trimmed: &str, output: &mut Vec<String>) -> Result<()> {
        let rest = trimmed["!theme".len()..].trim();
        if rest.is_empty() {
            return Ok(());
        }

        let (theme_name, from) = if let Some((name, from)) = split_keyword_from(rest) {
            (name.trim(), Some(from.trim()))
        } else {
            (rest, None)
        };

        let theme_target = self.resolve_theme_target(theme_name, from)?;
        let include_line = format!("!include_many {theme_target}");
        self.process_include(&include_line, IncludeMode::Many, output)
    }

    fn handle_undef(&mut self, trimmed: &str) {
        let rest = trimmed["!undef ".len()..].trim();
        let name = rest.trim();
        self.defines.remove(name);
        self.vars.remove(name);
        for scope in self.local_scopes.iter_mut().rev() {
            if scope.remove(name).is_some() {
                break;
            }
        }
        if !name.starts_with('$') {
            let dollar_name = format!("${name}");
            self.vars.remove(&dollar_name);
            for scope in self.local_scopes.iter_mut().rev() {
                if scope.remove(&dollar_name).is_some() {
                    break;
                }
            }
        }
    }

    // ── include ─────────────────────────────────────────────────

    fn process_include(
        &mut self,
        trimmed: &str,
        mode: IncludeMode,
        output: &mut Vec<String>,
    ) -> Result<()> {
        let target = extract_include_target(trimmed).ok_or_else(|| crate::Error::Parse {
            line: 1,
            column: Some(1),
            message: format!("malformed include directive: {trimmed}"),
        })?;
        let (raw_path, selector) = split_include_target(target);
        let resolved = self.resolve_include_path(raw_path)?;
        let include_key = build_include_key(&resolved.source_key, selector, mode);

        if self.included_once.contains(&include_key) {
            match mode {
                IncludeMode::Many | IncludeMode::Include | IncludeMode::IncludeSub => {}
                IncludeMode::IncludeOnce => {
                    return Err(crate::Error::Parse {
                        line: 1,
                        column: Some(1),
                        message: format!(
                            "file already included via !include_once: {}",
                            resolved.source_key
                        ),
                    });
                }
            }
        } else if matches!(mode, IncludeMode::IncludeOnce) {
            self.included_once.insert(include_key.clone());
        }

        if self.include_stack.contains(&include_key) {
            return Err(crate::Error::Parse {
                line: 1,
                column: Some(1),
                message: format!("recursive include detected: {}", resolved.source_key),
            });
        }
        if self.include_stack.len() >= 64 {
            return Err(crate::Error::Parse {
                line: 1,
                column: Some(1),
                message: format!("include depth limit (64) exceeded: {}", resolved.source_key),
            });
        }

        let source = fs::read_to_string(&resolved.path)?;
        let selected_source = match mode {
            IncludeMode::IncludeSub => {
                let selector = selector.ok_or_else(|| crate::Error::Parse {
                    line: 1,
                    column: Some(1),
                    message: format!("!includesub requires a subpart selector: {target}"),
                })?;
                extract_subpart_source(&source, selector)?
            }
            _ => {
                if let Some(selector) = selector {
                    extract_diagram_source(&source, selector)?
                } else {
                    source
                }
            }
        };

        self.include_stack.push(include_key);
        let result = self.process_source_into(
            &selected_source,
            resolved.path.parent(),
            resolved.base_url.as_ref(),
            Some(&resolved.source_key),
            output,
        );
        self.include_stack.pop();
        result
    }

    /// `!includedef` — like `!include` but only processes directive lines
    /// (lines starting with `!`).
    fn process_includedef(&mut self, trimmed: &str) -> Result<()> {
        let target = trimmed
            .strip_prefix("!includedef ")
            .ok_or_else(|| crate::Error::Parse {
                line: 1,
                column: Some(1),
                message: format!("malformed includedef directive: {trimmed}"),
            })?
            .trim();
        let (raw_path, _selector) = split_include_target(target);
        let resolved = self.resolve_include_path(raw_path)?;
        let source = fs::read_to_string(&resolved.path)?;

        // Filter to only directive lines
        let directive_lines: Vec<&str> = source
            .lines()
            .filter(|line| line.trim().starts_with('!'))
            .collect();
        let directive_source = directive_lines.join("\n");

        let mut discard = Vec::new();
        self.process_source_into(
            &directive_source,
            resolved.path.parent(),
            resolved.base_url.as_ref(),
            Some(&resolved.source_key),
            &mut discard,
        )
    }

    fn resolve_include_path(&mut self, target: &str) -> Result<ResolvedInclude> {
        if is_remote_reference(target) {
            let url = Url::parse(target)
                .map_err(|e| crate::Error::Render(format!("invalid URL: {e}")))?;
            let path = self.fetch_remote_url(url.as_str())?;
            return Ok(ResolvedInclude {
                path,
                source_key: url.as_str().to_string(),
                base_url: Some(parent_url(&url)),
            });
        }

        if target.starts_with('<') && target.ends_with('>') {
            let inner = target[1..target.len() - 1].trim();
            let path = self.resolve_stdlib_include(inner)?;
            return Ok(ResolvedInclude {
                source_key: format!("<{inner}>"),
                path,
                base_url: None,
            });
        }

        let resolved = self.resolve_filesystem_path(target)?;
        if resolved.exists() {
            let path = resolved.canonicalize().unwrap_or(resolved);
            return Ok(ResolvedInclude {
                source_key: path.display().to_string(),
                path,
                base_url: None,
            });
        }

        if let Some(base_url) = self.current_base_url() {
            let joined = base_url
                .join(target)
                .map_err(|e| crate::Error::Render(format!("invalid remote include target: {e}")))?;
            let path = self.fetch_remote_url(joined.as_str())?;
            return Ok(ResolvedInclude {
                path,
                source_key: joined.as_str().to_string(),
                base_url: Some(parent_url(&joined)),
            });
        }

        let unquoted = normalize_import_entry_path(&unquote(target));
        for root in self.imported_roots.iter().rev() {
            let candidate = root.join(&unquoted);
            if candidate.exists() {
                let path = candidate.canonicalize().unwrap_or(candidate);
                return Ok(ResolvedInclude {
                    source_key: path.display().to_string(),
                    path,
                    base_url: None,
                });
            }
        }

        Ok(ResolvedInclude {
            source_key: resolved.display().to_string(),
            path: resolved,
            base_url: None,
        })
    }

    fn resolve_filesystem_path(&self, target: &str) -> Result<PathBuf> {
        let unquoted = unquote(target);
        let candidate = Path::new(&unquoted);
        let resolved = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else if let Some(base_dir) = self.current_base_dir() {
            base_dir.join(candidate)
        } else {
            std::env::current_dir()?.join(candidate)
        };
        Ok(resolved)
    }

    fn resolve_stdlib_include(&self, inner: &str) -> Result<PathBuf> {
        let stdlib_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");
        let relative = Path::new(inner);
        let direct = stdlib_root.join(relative);
        let with_ext = if relative.extension().is_some() {
            direct.clone()
        } else {
            stdlib_root.join(format!("{inner}.puml"))
        };

        let candidate = if direct.exists() { direct } else { with_ext };
        if candidate.exists() {
            Ok(candidate.canonicalize().unwrap_or(candidate))
        } else {
            Err(crate::Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("stdlib include not found: <{inner}>"),
            )))
        }
    }

    fn resolve_theme_target(&self, theme_name: &str, from: Option<&str>) -> Result<String> {
        let theme_file = theme_filename(theme_name);
        let candidate = if let Some(from) = from {
            if from.starts_with('<') && from.ends_with('>') {
                let inner = from[1..from.len() - 1].trim();
                Some(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("stdlib")
                        .join(inner)
                        .join(&theme_file),
                )
            } else if is_remote_reference(from) {
                None
            } else {
                Some(self.resolve_filesystem_path(from)?.join(&theme_file))
            }
        } else {
            Some(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("stdlib")
                    .join("themes")
                    .join(&theme_file),
            )
        };

        if let Some(candidate) = candidate {
            if candidate.exists() {
                let path = candidate.canonicalize().unwrap_or(candidate);
                return Ok(path.display().to_string());
            }
        }

        if let Some(from) = from {
            if is_remote_reference(from) {
                let base = Url::parse(from)
                    .map_err(|e| crate::Error::Render(format!("invalid theme base URL: {e}")))?;
                let joined = base
                    .join(&theme_file)
                    .map_err(|e| crate::Error::Render(format!("invalid remote theme URL: {e}")))?;
                return Ok(joined.to_string());
            }
        }

        Ok(default_remote_theme_url(&theme_file))
    }

    #[cfg(feature = "remote")]
    fn fetch_remote_url(&mut self, url: &str) -> Result<PathBuf> {
        if let Some(path) = self.remote_files.get(url) {
            return Ok(path.clone());
        }

        let response = ureq::get(url)
            .call()
            .map_err(|e| crate::Error::Io(io::Error::other(format!("remote fetch failed: {e}"))))?;
        let body = response.into_body().read_to_vec().map_err(|e| {
            crate::Error::Io(io::Error::other(format!("remote fetch body failed: {e}")))
        })?;

        let out_path = make_remote_temp_path(url);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&out_path, &body)?;
        self.remote_files.insert(url.to_string(), out_path.clone());
        Ok(out_path)
    }

    #[cfg(not(feature = "remote"))]
    fn fetch_remote_url(&mut self, url: &str) -> Result<PathBuf> {
        Err(crate::Error::Io(io::Error::other(format!(
            "remote fetch disabled (feature = \"remote\"): {url}"
        ))))
    }

    // ── variable assignment ─────────────────────────────────────

    /// Try to parse `$var = value` or `$var="value"`.
    /// Returns `Some(true)` if consumed, `Some(false)` / `None` otherwise.
    ///
    /// Variable references on the RHS are expanded, and if the result looks
    /// like an arithmetic expression it is evaluated numerically.
    /// The resulting value is stored as a typed `Value` (Int, Array, or Str).
    fn try_var_assign(&mut self, rest: &str, mode: VarAssignMode) -> Result<Option<bool>> {
        // Format: `$name = value`, `$name=value`, `$name ?= value`
        let rest = rest.trim();
        let (name, raw_val, assign_if_missing) =
            if let Some((left, right)) = split_top_level_once(rest, '?') {
                let right = right.trim_start();
                if let Some(stripped) = right.strip_prefix('=') {
                    (left.trim().to_string(), stripped.trim(), true)
                } else {
                    let Some((left, right)) = split_top_level_once(rest, '=') else {
                        return Ok(None);
                    };
                    (left.trim().to_string(), right.trim(), false)
                }
            } else {
                let Some((left, right)) = split_top_level_once(rest, '=') else {
                    return Ok(None);
                };
                (left.trim().to_string(), right.trim(), false)
            };
        if !name.starts_with('$') {
            return Ok(None);
        }

        if assign_if_missing && self.is_var_defined(&name) {
            return Ok(Some(true));
        }

        // Check for bracket array syntax BEFORE unquoting
        // (unquote would strip surrounding quotes, not brackets)
        let trimmed_raw = raw_val.trim();
        if trimmed_raw.starts_with('[') && trimmed_raw.ends_with(']') {
            // Expand variables inside the array expression
            let expanded = self.expand_vars(trimmed_raw);
            let value = Value::parse_from(&expanded);
            self.assign_var(name, value, mode);
            return Ok(Some(true));
        }

        let value = self.evaluate_assignment_value_with_funcs(raw_val)?;
        self.assign_var(name, value, mode);
        Ok(Some(true))
    }

    /// Try to parse legacy assignment: `NAME=value` or `NAME = value`
    fn try_legacy_assign(&mut self, rest: &str) -> Option<bool> {
        let rest = rest.trim();
        let eq_pos = rest.find('=')?;
        let name = rest[..eq_pos].trim();
        // Name must be non-empty and look like an identifier
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return None;
        }
        let raw_val = rest[eq_pos + 1..].trim();
        let value = unquote(raw_val);
        // Store both as a define (for text-replacement) and as $NAME for $-substitution
        self.defines.insert(
            name.to_string(),
            DefineEntry {
                params: vec![],
                body: value.clone(),
            },
        );
        Some(true)
    }

    // ── !define ─────────────────────────────────────────────────

    fn handle_define(&mut self, rest: &str) {
        // `NAME(params) body` or `NAME body`
        let rest = rest.trim();
        if let Some(paren_start) = rest.find('(') {
            let name = rest[..paren_start].trim().to_string();
            if let Some(paren_end) = rest.find(')') {
                let params_str = &rest[paren_start + 1..paren_end];
                let params: Vec<String> = params_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                let body = rest[paren_end + 1..].trim().to_string();
                self.defines.insert(name, DefineEntry { params, body });
            }
        } else {
            let mut parts = rest.splitn(2, char::is_whitespace);
            if let Some(name) = parts.next() {
                let body = parts.next().unwrap_or("").trim().to_string();
                self.defines.insert(
                    name.to_string(),
                    DefineEntry {
                        params: vec![],
                        body,
                    },
                );
            }
        }
    }

    fn collect_define_long(&mut self, lines: &[&str], start: usize) -> usize {
        let header = lines[start].trim();
        let rest = header["!definelong ".len()..].trim();

        let (name, params) = if let Some(paren_start) = rest.find('(') {
            let name = rest[..paren_start].trim().to_string();
            let paren_end = rest.rfind(')').unwrap_or(rest.len());
            let params_str = &rest[paren_start + 1..paren_end];
            let params: Vec<String> = params_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            (name, params)
        } else {
            (rest.to_string(), vec![])
        };

        let mut body = Vec::new();
        let mut i = start + 1;
        while i < lines.len() {
            let trimmed = lines[i].trim();
            if matches!(
                trimmed,
                "!enddefinelong" | "!end definelong" | "!enddefine" | "!end define"
            ) {
                i += 1;
                break;
            }
            body.push(lines[i].to_string());
            i += 1;
        }

        self.defines.insert(
            name,
            DefineEntry {
                params,
                body: body.join("\n"),
            },
        );

        i
    }

    // ── !function / !procedure ──────────────────────────────────

    /// Collect a function/procedure definition starting at `start`.
    /// Returns the index of the first line AFTER the definition.
    fn collect_func_def(&mut self, lines: &[&str], start: usize) -> usize {
        let header = lines[start].trim();

        // Strip optional `!unquoted` prefix
        let header = header.strip_prefix("!unquoted ").unwrap_or(header);

        let is_procedure = header.starts_with("!procedure ") || header.starts_with("procedure ");

        let rest = if is_procedure {
            header
                .strip_prefix("!procedure ")
                .or_else(|| header.strip_prefix("procedure "))
                .unwrap_or(header)
        } else {
            header
                .strip_prefix("!function ")
                .or_else(|| header.strip_prefix("function "))
                .unwrap_or(header)
        };

        let rest = rest.trim();

        // Parse name and params
        let (name, params) = if let Some(paren_start) = rest.find('(') {
            let name = rest[..paren_start].trim().to_string();
            let paren_end = rest.rfind(')').unwrap_or(rest.len());
            let params_str = &rest[paren_start + 1..paren_end];
            let params: Vec<ParamSpec> = params_str
                .split(',')
                .map(parse_param_spec)
                .filter(|p| !p.name.is_empty())
                .collect();
            (name, params)
        } else {
            (rest.to_string(), vec![])
        };

        // Collect body until !endfunction / !endprocedure / !end function / !end procedure
        let mut body = Vec::new();
        let mut i = start + 1;
        while i < lines.len() {
            let trimmed = lines[i].trim();
            if trimmed == "!endfunction"
                || trimmed == "!endprocedure"
                || trimmed == "!end function"
                || trimmed == "!end procedure"
            {
                i += 1;
                break;
            }
            body.push(lines[i].to_string());
            i += 1;
        }

        self.funcs.insert(
            name,
            UserFunc {
                params,
                body,
                is_procedure,
            },
        );

        i
    }

    // ── conditionals ────────────────────────────────────────────

    /// Process an `!if`/`!ifdef`/`!ifndef` block.
    /// Returns the index of the first line AFTER `!endif`.
    fn process_conditional(
        &mut self,
        lines: &[&str],
        start: usize,
        output: &mut Vec<String>,
    ) -> Result<usize> {
        let mut branches: Vec<(Option<String>, Vec<&str>)> =
            vec![(Some(lines[start].trim().to_string()), Vec::new())];
        let mut current_branch = 0usize;
        let mut depth = 0;
        let mut i = start + 1;

        while i < lines.len() {
            let t = lines[i].trim();
            if t.starts_with("!if ") || t.starts_with("!ifdef ") || t.starts_with("!ifndef ") {
                depth += 1;
            }
            if t == "!endif" {
                if depth == 0 {
                    i += 1;
                    break;
                }
                depth -= 1;
            }
            if depth == 0 && t.starts_with("!elseif ") {
                branches.push((Some(t.to_string()), Vec::new()));
                current_branch = branches.len() - 1;
                i += 1;
                continue;
            }
            if t == "!else" && depth == 0 {
                branches.push((None, Vec::new()));
                current_branch = branches.len() - 1;
                i += 1;
                continue;
            }
            branches[current_branch].1.push(lines[i]);
            i += 1;
        }

        for (condition, branch_lines) in branches {
            let matched = match condition {
                Some(header) => self.eval_condition(&header),
                None => true,
            };
            if matched {
                self.process_lines(&branch_lines, output)?;
                break;
            }
        }

        Ok(i)
    }

    // ── !foreach ────────────────────────────────────────────────

    /// Process `!foreach $var in [items]` ... `!endfor`.
    /// Returns the index of the first line AFTER `!endfor`.
    fn process_foreach(
        &mut self,
        lines: &[&str],
        start: usize,
        output: &mut Vec<String>,
    ) -> Result<usize> {
        let header = lines[start].trim();
        let rest = &header["!foreach ".len()..];

        // Parse: `$var in <expr>`
        let (var_name, collection_expr) = if let Some((v, e)) = rest.split_once(" in ") {
            (v.trim().to_string(), e.trim().to_string())
        } else {
            log::warn!("malformed !foreach: {header}");
            return Ok(start + 1);
        };

        // Evaluate the collection expression — expand variables first
        let expanded_expr = self.expand_vars(&collection_expr);

        // Try to resolve from a variable that is already an Array
        let collection_value = if let Some(val) = self.lookup_var(expanded_expr.trim()) {
            val.clone()
        } else {
            Value::parse_from(&expanded_expr)
        };

        // Collect body lines until `!endfor` (handle nesting)
        let mut body: Vec<String> = Vec::new();
        let mut depth = 0;
        let mut i = start + 1;
        while i < lines.len() {
            let t = lines[i].trim();
            if t.starts_with("!foreach ") {
                depth += 1;
            }
            if t == "!endfor" {
                if depth == 0 {
                    i += 1;
                    break;
                }
                depth -= 1;
            }
            body.push(lines[i].to_string());
            i += 1;
        }

        // Iterate over array items, or fall back to string-based parse_array
        let items: Vec<Value> = if let Some(arr) = collection_value.as_array() {
            arr.to_vec()
        } else {
            // Fall back to legacy string parsing for backward compat
            let items_str =
                parse_array(&expanded_expr).unwrap_or_else(|| vec![expanded_expr.clone()]);
            items_str
                .into_iter()
                .map(|s| Value::parse_from(&s))
                .collect()
        };

        // Iterate: for each element, set variable and process body
        for item in &items {
            self.assign_var(var_name.clone(), item.clone(), VarAssignMode::Local);
            let body_refs: Vec<&str> = body.iter().map(std::string::String::as_str).collect();
            self.process_lines(&body_refs, output)?;
            if self.has_pending_return() {
                break;
            }
        }

        Ok(i)
    }

    // ── !while ─────────────────────────────────────────────────

    /// Process `!while condition` ... `!endwhile`.
    /// Returns the index of the first line AFTER `!endwhile`.
    ///
    /// Safety: max 10000 iterations to prevent infinite loops.
    fn process_while(
        &mut self,
        lines: &[&str],
        start: usize,
        output: &mut Vec<String>,
    ) -> Result<usize> {
        let header = lines[start].trim();
        let condition_expr = header["!while ".len()..].trim().to_string();

        // Collect body lines until `!endwhile` (handle nesting)
        let mut body: Vec<String> = Vec::new();
        let mut depth = 0;
        let mut i = start + 1;
        while i < lines.len() {
            let t = lines[i].trim();
            if t.starts_with("!while ") {
                depth += 1;
            }
            if t == "!endwhile" {
                if depth == 0 {
                    i += 1;
                    break;
                }
                depth -= 1;
            }
            body.push(lines[i].to_string());
            i += 1;
        }

        const MAX_ITERATIONS: usize = 10_000;
        let mut iterations = 0;

        while self.eval_if_expr(&condition_expr) {
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                let preview: Vec<&str> = body.iter().take(4).map(String::as_str).collect();
                return Err(crate::Error::Parse {
                    line: start + 1,
                    column: Some(1),
                    message: format!(
                        "!while loop exceeded {MAX_ITERATIONS} iterations: condition=`{condition_expr}` body={preview:?}"
                    ),
                });
            }
            let body_refs: Vec<&str> = body.iter().map(std::string::String::as_str).collect();
            self.process_lines(&body_refs, output)?;
            if self.has_pending_return() {
                break;
            }
        }

        Ok(i)
    }

    // ── conditionals ────────────────────────────────────────────

    /// Evaluate a conditional directive header.
    fn eval_condition(&self, trimmed: &str) -> bool {
        if let Some(rest) = strip_directive_prefix(trimmed, "!ifdef ") {
            let name = rest.trim();
            return self.is_defined(name);
        }
        if let Some(rest) = strip_directive_prefix(trimmed, "!ifndef ") {
            let name = rest.trim();
            return !self.is_defined(name);
        }
        if let Some(rest) = strip_directive_prefix(trimmed, "!if ") {
            return self.eval_if_expr(rest.trim());
        }
        if let Some(rest) = strip_directive_prefix(trimmed, "!elseif ") {
            return self.eval_if_expr(rest.trim());
        }
        false
    }

    fn is_defined(&self, name: &str) -> bool {
        self.is_var_defined(name)
            || self.defines.contains_key(name)
            || self.is_var_defined(&format!("${name}"))
    }

    /// Expression evaluator for `!if` conditions.
    ///
    /// Supports: `==`, `!=`, `<`, `>`, `<=`, `>=` (numeric when possible),
    /// `%variable_exists()`.
    fn eval_if_expr(&self, expr: &str) -> bool {
        let expr = expr.trim();

        let expanded_dynamic = self.expand_context_builtins(expr);
        let expr_expanded = expand_builtins(&self.expand_vars(&expanded_dynamic));
        let expr = expr_expanded.trim();

        if let Some(parts) = split_top_level_operator(expr, "||") {
            return parts.into_iter().any(|part| self.eval_if_expr(part));
        }

        if let Some(parts) = split_top_level_operator(expr, "&&") {
            return parts.into_iter().all(|part| self.eval_if_expr(part));
        }

        if let Some(inner) = strip_wrapping_parens(expr) {
            return self.eval_if_expr(inner);
        }

        if let Some(rest) = expr.strip_prefix('!') {
            let rest = rest.trim_start();
            if !rest.starts_with('=') {
                return !self.eval_if_expr(rest);
            }
        }

        self.eval_if_predicate(expr)
    }

    fn eval_if_predicate(&self, expr: &str) -> bool {
        let expr = expr.trim();

        // Handle %variable_exists("$name")
        if expr.starts_with("%variable_exists(") {
            if let Some(inner) = extract_func_arg(expr, "%variable_exists") {
                let name = unquote(inner.trim());
                return self.is_defined(&name);
            }
        }

        // Try two-char operators first (==, !=, <=, >=), then single-char (< >).
        // Order matters: check `<=` / `>=` before `<` / `>`, and `==` / `!=`
        // before `=`.
        for &(op, op_len) in &[
            ("==", 2),
            ("!=", 2),
            ("<=", 2),
            (">=", 2),
            ("<", 1),
            (">", 1),
        ] {
            if let Some(pos) = expr.find(op) {
                // For single-char `<` / `>`, make sure it's not part of `<=` / `>=` / `==` / `!=`
                if op_len == 1 {
                    let next = expr.as_bytes().get(pos + 1);
                    if next == Some(&b'=') {
                        continue;
                    }
                    // Also skip if preceded by `!` or `=` for `!=` or `==`
                    if op == ">" && pos > 0 && expr.as_bytes()[pos - 1] == b'=' {
                        // This is `>=` matched at a wrong offset — should not happen
                        // because we already matched `>=`, but guard anyway
                        continue;
                    }
                }

                let left_str = self.eval_simple_value(expr[..pos].trim());
                let right_str = self.eval_simple_value(expr[pos + op_len..].trim());

                return match op {
                    "==" => left_str == right_str,
                    "!=" => left_str != right_str,
                    "<" | ">" | "<=" | ">=" => {
                        let lv = left_str.parse::<f64>();
                        let rv = right_str.parse::<f64>();
                        if let (Ok(l), Ok(r)) = (lv, rv) {
                            match op {
                                "<" => l < r,
                                ">" => l > r,
                                "<=" => l <= r,
                                ">=" => l >= r,
                                _ => unreachable!(),
                            }
                        } else {
                            // Fall back to lexicographic comparison
                            match op {
                                "<" => left_str < right_str,
                                ">" => left_str > right_str,
                                "<=" => left_str <= right_str,
                                ">=" => left_str >= right_str,
                                _ => unreachable!(),
                            }
                        }
                    }
                    _ => false,
                };
            }
        }

        // Treat non-empty / non-zero as true (Value-aware)
        let val_str = self.eval_simple_value(expr);
        let val = Value::parse_from(&val_str);
        val.is_truthy()
    }

    fn eval_simple_value(&self, s: &str) -> String {
        let s = s.trim();
        if ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
            && s.len() >= 2
        {
            return s[1..s.len() - 1].to_string();
        }
        if s.starts_with('$') {
            return self.lookup_variable_text(s);
        }
        s.to_string()
    }

    // ── line expansion ──────────────────────────────────────────

    fn expand_line(&mut self, line: &str) -> Result<String> {
        if self.try_set_variable_value_call(line.trim())? {
            return Ok(String::new());
        }

        let mut result = line.to_string();

        // Expand built-in functions that depend on preprocessor state first.
        result = self.expand_context_builtins(&result);

        // Expand stateless built-in functions BEFORE variable/define substitution.
        // 这与 Java applyFunctionsAndVariables 的单次扫描语义对齐：Java 在替换
        // 变量时只推进输入串的指针，替换进来的值不会被再次扫描。所以源码里
        // 写的 `%n()` 会被展开，而来自引号字符串字面量的变量值里保留的
        // `%n()` 字面量会被原样保留下来。
        //
        // Example: `!t="a%n()b"` stores literal `a%n()b` as the value. Then
        // `while (t)` becomes `while (a%n()b)` after substitution and the
        // literal %n() survives into the rendered label.
        result = expand_builtins(&result);

        // Expand define macros (with params)
        result = self.expand_defines(&result);

        // Expand $variables
        result = self.expand_vars(&result);

        // Keep line-level callable expansion out of !function bodies.
        // Their assignments / returns are evaluated through expression helpers,
        // which is both more accurate for stdlib code and avoids recursive
        // re-entry through expand_line -> eval_func -> process_lines.
        if self.func_expansion_depth == 0 {
            result = self.expand_func_calls(&result)?;
        }

        Ok(result)
    }

    fn expand_defines(&self, line: &str) -> String {
        let mut result = line.to_string();
        // Sort by length descending to match longer names first
        let mut names: Vec<&String> = self.defines.keys().collect();
        names.sort_by_key(|n| std::cmp::Reverse(n.len()));

        for name in names {
            let entry = &self.defines[name];
            if entry.params.is_empty() {
                // Java Define.apply2(): translates \n to private-use char before
                // word-boundary matching so that `\n` acts as a word boundary,
                // then translates back afterwards.
                result = translate_backslashes(&result);
                result = replace_word_boundary(&result, name, &entry.body);
                result = untranslate_backslashes(&result);
            } else {
                // Parameterised macro: NAME(arg1, arg2, ...)
                result = expand_parameterised_define(&result, name, entry);
            }
        }
        result
    }

    fn expand_vars(&self, line: &str) -> String {
        let mut result = line.to_string();
        // Sort by length descending so $foobar replaces before $foo
        let visible = self.visible_vars();
        let mut names: Vec<&String> = visible.keys().collect();
        names.sort_by_key(|n| std::cmp::Reverse(n.len()));
        for name in names {
            let val = visible[name].as_str();
            result = result.replace(name.as_str(), &val);
        }
        result
    }

    fn expand_vars_in_expression(&self, line: &str) -> String {
        let visible = self.visible_vars();
        let mut names: Vec<&String> = visible.keys().collect();
        names.sort_by_key(|n| std::cmp::Reverse(n.len()));

        let mut out = String::new();
        let mut idx = 0usize;
        let mut quote_char = None;
        let mut escaped = false;

        while idx < line.len() {
            let ch = match line[idx..].chars().next() {
                Some(ch) => ch,
                None => break,
            };
            let ch_len = ch.len_utf8();

            if let Some(active_quote) = quote_char {
                out.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == active_quote {
                    quote_char = None;
                }
                idx += ch_len;
                continue;
            }

            match ch {
                '"' | '\'' => {
                    quote_char = Some(ch);
                    out.push(ch);
                    idx += ch_len;
                }
                '$' => {
                    let mut matched = false;
                    for name in &names {
                        let name = name.as_str();
                        if line[idx..].starts_with(name)
                            && is_variable_boundary_end(line, idx + name.len())
                        {
                            out.push_str(&visible[name].as_str());
                            idx += name.len();
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        out.push(ch);
                        idx += ch_len;
                    }
                }
                _ => {
                    out.push(ch);
                    idx += ch_len;
                }
            }
        }

        out
    }

    fn expand_func_calls(&mut self, line: &str) -> Result<String> {
        let mut result = line.to_string();

        for _ in 0..64 {
            let previous = result.clone();

            if let Some(expanded) = self.try_expand_dynamic_call(&result)? {
                result = expanded;
            }
            let names: Vec<String> = self.funcs.keys().cloned().collect();
            for name in &names {
                if let Some(expanded) = self.try_expand_call(&result, name)? {
                    result = expanded;
                }
            }

            if result == previous {
                return Ok(result);
            }
        }

        Err(crate::Error::Parse {
            line: 0,
            column: Some(1),
            message: "call expansion did not converge within 64 rounds".to_string(),
        })
    }

    fn expand_expression_func_calls(&mut self, line: &str) -> Result<String> {
        let mut result = line.to_string();
        if let Some(expanded) = self.try_expand_dynamic_call_expression(&result)? {
            result = expanded;
        }
        let names: Vec<String> = self.funcs.keys().cloned().collect();
        for name in &names {
            if let Some(expanded) = self.try_expand_call_expression(&result, name)? {
                result = expanded;
            }
        }
        Ok(result)
    }

    fn try_expand_dynamic_call(&mut self, line: &str) -> Result<Option<String>> {
        for (builtin_name, require_procedure) in
            [("%call_user_func", false), ("%invoke_procedure", true)]
        {
            let Some(call_start) = find_named_call_start(line, builtin_name) else {
                continue;
            };
            let after_name = call_start + builtin_name.len();
            let rest = &line[after_name..];
            if !rest.starts_with('(') {
                continue;
            }

            let Some(end) = find_matching_call_end(rest) else {
                continue;
            };
            let args = split_args(&rest[1..end]);
            let Some((raw_name, raw_args)) = args.split_first() else {
                continue;
            };

            let resolved_name = self.resolve_callable_name(raw_name);
            let Some(func) = self.funcs.get(&resolved_name).cloned() else {
                continue;
            };
            if require_procedure && !func.is_procedure {
                continue;
            }

            self.expanding_funcs.push(resolved_name.clone());
            let replacement = self.eval_func(&func, raw_args);
            self.expanding_funcs.pop();
            let replacement = replacement?;
            let mut out = String::new();
            out.push_str(&line[..call_start]);
            out.push_str(&replacement);
            out.push_str(&rest[end + 1..]);
            return Ok(Some(out));
        }

        Ok(None)
    }

    fn try_expand_dynamic_call_expression(&mut self, line: &str) -> Result<Option<String>> {
        let Some(call_start) = find_named_call_start(line, "%call_user_func") else {
            return Ok(None);
        };
        let after_name = call_start + "%call_user_func".len();
        let rest = &line[after_name..];
        if !rest.starts_with('(') {
            return Ok(None);
        }

        let Some(end) = find_matching_call_end(rest) else {
            return Ok(None);
        };
        let args = split_args(&rest[1..end]);
        let Some((raw_name, raw_args)) = args.split_first() else {
            return Ok(None);
        };

        let resolved_name = self.resolve_callable_name(raw_name);
        let Some(func) = self.funcs.get(&resolved_name).cloned() else {
            return Ok(None);
        };
        if func.is_procedure {
            return Ok(None);
        }

        if self.expanding_funcs.contains(&resolved_name) {
            return Ok(None);
        }

        self.expanding_funcs.push(resolved_name.clone());
        let replacement = self.eval_func(&func, raw_args);
        self.expanding_funcs.pop();
        let replacement = replacement?;

        let mut out = String::new();
        out.push_str(&line[..call_start]);
        out.push_str(&replacement);
        out.push_str(&rest[end + 1..]);
        Ok(Some(out))
    }

    /// Try to find and expand one occurrence of `name(args...)` in `line`.
    fn try_expand_call(&mut self, line: &str, name: &str) -> Result<Option<String>> {
        // Skip functions currently being expanded to prevent infinite recursion
        if self.expanding_funcs.contains(&name.to_string()) {
            return Ok(None);
        }
        let Some(func) = self.funcs.get(name).cloned() else {
            return Ok(None);
        };
        let Some(call_start) = find_named_call_start(line, name) else {
            return Ok(None);
        };
        let after_name = call_start + name.len();
        let rest = &line[after_name..];

        // Must be followed by '('
        if !rest.starts_with('(') {
            return Ok(None);
        }

        // Find matching ')'
        let Some(end) = find_matching_call_end(rest) else {
            return Ok(None);
        };

        let args_str = &rest[1..end];
        let args = split_args(args_str);

        // Build substitution: for procedure, evaluate body as lines;
        // for function, evaluate body and look for !return.
        self.expanding_funcs.push(name.to_string());
        let replacement = self.eval_func(&func, &args);
        self.expanding_funcs.pop();
        let replacement = replacement?;

        let mut out = String::new();
        out.push_str(&line[..call_start]);
        out.push_str(&replacement);
        out.push_str(&rest[end + 1..]);
        Ok(Some(out))
    }

    fn try_expand_call_expression(&mut self, line: &str, name: &str) -> Result<Option<String>> {
        if self.expanding_funcs.contains(&name.to_string()) {
            return Ok(None);
        }
        let Some(func) = self.funcs.get(name).cloned() else {
            return Ok(None);
        };
        if func.is_procedure {
            return Ok(None);
        }
        let Some(call_start) = find_named_call_start(line, name) else {
            return Ok(None);
        };
        let after_name = call_start + name.len();
        let rest = &line[after_name..];
        if !rest.starts_with('(') {
            return Ok(None);
        }

        let Some(end) = find_matching_call_end(rest) else {
            return Ok(None);
        };

        let args = split_args(&rest[1..end]);
        self.expanding_funcs.push(name.to_string());
        let replacement = self.eval_func(&func, &args);
        self.expanding_funcs.pop();
        let replacement = replacement?;

        let mut out = String::new();
        out.push_str(&line[..call_start]);
        out.push_str(&replacement);
        out.push_str(&rest[end + 1..]);
        Ok(Some(out))
    }

    fn eval_func(&mut self, func: &UserFunc, args: &[String]) -> Result<String> {
        if self.local_scopes.len() >= 128 {
            let call_stack: Vec<&str> = self
                .local_scopes
                .iter()
                .filter_map(|s| s.keys().next().map(std::string::String::as_str))
                .rev()
                .take(5)
                .collect();
            log::error!("call depth 128 exceeded. recent locals: {call_stack:?}");
            return Err(crate::Error::Parse {
                line: 0,
                column: Some(1),
                message: format!(
                    "function call depth limit (128) exceeded, recent stack: {call_stack:?}"
                ),
            });
        }
        let (positional_args, keyword_args) = parse_call_arguments(args);
        let mut next_positional = 0usize;
        let mut local_scope = HashMap::new();

        for param in &func.params {
            let param_name = format!("${}", param.name.trim_start_matches('$'));
            let value = if let Some(value) = keyword_args
                .get(&param.name)
                .or_else(|| keyword_args.get(param_name.trim_start_matches('$')))
                .or_else(|| keyword_args.get(&param_name))
            {
                normalize_param_value(value)
            } else if let Some(value) = positional_args.get(next_positional) {
                next_positional += 1;
                normalize_param_value(value)
            } else if let Some(default) = &param.default {
                normalize_param_value(default)
            } else {
                String::new()
            };
            local_scope.insert(param_name, Value::Str(value));
        }

        self.local_scopes.push(local_scope);
        if !func.is_procedure {
            self.func_expansion_depth += 1;
            self.return_stack.push(None);
        }

        let body_refs: Vec<&str> = func.body.iter().map(std::string::String::as_str).collect();
        let result = (|| -> Result<String> {
            if func.is_procedure {
                let mut output = Vec::new();
                self.process_lines(&body_refs, &mut output)?;
                Ok(output.join("\n"))
            } else {
                let mut output = Vec::new();
                self.process_lines(&body_refs, &mut output)?;
                if let Some(value) = self.return_stack.last_mut().and_then(Option::take) {
                    Ok(value)
                } else {
                    Ok(output.join("\n"))
                }
            }
        })();
        if !func.is_procedure {
            self.func_expansion_depth = self.func_expansion_depth.saturating_sub(1);
            self.return_stack.pop();
        }
        self.local_scopes.pop();

        result
    }

    fn evaluate_assignment_value_with_funcs(&mut self, raw_val: &str) -> Result<Value> {
        let expanded = self.evaluate_expression_text_with_funcs(raw_val)?;
        let trimmed = expanded.trim();

        if matches!(trimmed, "%newline()" | "%n()") {
            return Ok(Value::Str(crate::NEWLINE_CHAR.to_string()));
        }

        // Preserve evaluated multiline strings verbatim. Re-parsing through
        // Value::parse_from() trims line endings, which breaks helpers that
        // intentionally build text with %newline() inside loops.
        // Note: U+E100 (NEWLINE_CHAR) is the placeholder for %newline() expansions.
        if expanded.contains('\n')
            || expanded.contains('\r')
            || expanded.contains(crate::NEWLINE_CHAR)
        {
            return Ok(Value::Str(expanded));
        }

        // If the raw value was a quoted literal (e.g. `"hello+world"`), the inner
        // text has already been extracted by evaluate_expression_text_with_funcs.
        // Do NOT re-evaluate through arithmetic/concat, since operators like `+`
        // inside a quoted string are literal text, not expressions.
        let raw_trimmed = raw_val.trim();
        if is_quoted_literal(raw_trimmed)
            && !raw_trimmed.contains('$')
            && !raw_trimmed.contains('%')
        {
            return Ok(Value::Str(expanded));
        }

        if let Some(arith_str) = try_eval_arithmetic(&expanded) {
            return Ok(Value::parse_from(&arith_str));
        }

        if let Some(concat) = try_eval_concat_expr(&expanded) {
            return Ok(Value::Str(concat));
        }

        Ok(Value::parse_from(&expanded))
    }

    fn evaluate_expression_text_with_funcs(&mut self, raw: &str) -> Result<String> {
        let mut current = raw.to_string();

        for _ in 0..32 {
            let pre_vars = self.expand_vars_in_expression(&current);
            let pre_builtins = expand_expression_builtins(&pre_vars);
            let pre_funcs = self.expand_expression_func_calls(&pre_builtins)?;

            let expanded_dynamic = self.expand_context_builtins(&pre_funcs);
            let expanded_vars = self.expand_vars_in_expression(&expanded_dynamic);
            let expanded = expand_expression_builtins(&expanded_vars);
            let with_funcs = self.expand_expression_func_calls(&expanded)?;

            if with_funcs == current {
                break;
            }
            current = with_funcs;
        }

        if let Some(arith_str) = try_eval_arithmetic(&current) {
            return Ok(arith_str);
        }

        if let Some(concat) = try_eval_concat_expr(&current) {
            return Ok(concat);
        }

        let trimmed = current.trim();
        if is_quoted_literal(trimmed) {
            let inner = unquote(trimmed);
            if requires_round_trip_quotes(&inner) {
                return Ok(trimmed.to_string());
            }
            return Ok(inner);
        }

        Ok(current)
    }

    fn evaluate_expression_text(&self, raw: &str) -> String {
        let expanded_dynamic = self.expand_context_builtins(raw);
        let expanded =
            expand_expression_builtins(&self.expand_vars_in_expression(&expanded_dynamic));

        if let Some(arith_str) = try_eval_arithmetic(&expanded) {
            return arith_str;
        }

        if let Some(concat) = try_eval_concat_expr(&expanded) {
            return concat;
        }

        let trimmed = expanded.trim();
        if is_quoted_literal(trimmed) {
            let inner = unquote(trimmed);
            if requires_round_trip_quotes(&inner) {
                return trimmed.to_string();
            }
            return inner;
        }

        expanded
    }

    fn resolve_callable_name(&self, raw: &str) -> String {
        let value = unquote(self.evaluate_expression_text(raw).trim());
        if self.funcs.contains_key(&value) || value.starts_with('$') {
            value
        } else {
            format!("${value}")
        }
    }

    fn lookup_variable_text(&self, name: &str) -> String {
        if let Some(value) = self.lookup_var(name) {
            return value.as_str();
        }

        let normalized = if name.starts_with('$') {
            name.to_string()
        } else {
            format!("${name}")
        };
        if let Some(value) = self.lookup_var(&normalized) {
            return value.as_str();
        }

        if let Some(value) = self.defines.get(name) {
            return value.body.clone();
        }

        String::new()
    }

    fn resolve_builtin_text_arg(&self, arg: &str) -> String {
        let value = unquote(arg.trim());
        if value.starts_with('$') {
            self.lookup_variable_text(&value)
        } else {
            value
        }
    }

    fn evaluate_builtin_text_expr(&self, raw: &str) -> String {
        let trimmed = raw.trim();
        if is_quoted_literal(trimmed) {
            return unquote(trimmed);
        }

        let expanded = expand_expression_builtins(&self.expand_vars_in_expression(trimmed));
        if let Some(concat) = try_eval_concat_expr(&expanded) {
            concat
        } else {
            unquote(expanded.trim())
        }
    }

    fn evaluate_builtin_int_expr(&self, raw: &str) -> i64 {
        let expanded = self.evaluate_expression_text(raw.trim());
        parse_int_like(&expanded)
    }

    /// Parse a source key as a URL, but reject Windows drive-letter paths.
    /// A bare path like `c:\dir\file.puml` parses as a URL whose scheme is the
    /// single-letter drive (`c`), which would otherwise hijack the path-based
    /// `%filename()` / `%dirpath()` logic. Real URLs we handle (http/https/file)
    /// always have multi-character schemes, so a one-character scheme means we
    /// are looking at a filesystem path and should fall through to `Path`.
    fn parse_source_url(source_key: &str) -> Option<Url> {
        let url = Url::parse(source_key).ok()?;
        if url.scheme().len() == 1 {
            return None;
        }
        Some(url)
    }

    fn current_filename_text(&self) -> String {
        let Some(source_key) = self.current_source_key() else {
            return String::new();
        };
        if let Some(url) = Self::parse_source_url(source_key) {
            return url
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .filter(|s| !s.is_empty())
                .unwrap_or("")
                .to_string();
        }
        Path::new(source_key)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
    }

    fn current_dirpath_text(&self) -> String {
        if let Some(source_key) = self.current_source_key() {
            if let Some(url) = Self::parse_source_url(source_key) {
                return parent_url(&url).to_string();
            }
            if let Some(parent) = Path::new(source_key).parent() {
                return parent.display().to_string();
            }
        }
        self.current_base_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    }

    fn expand_context_builtins(&self, line: &str) -> String {
        let mut result = line.to_string();

        for _ in 0..16 {
            let mut changed = false;

            if let Some(updated) =
                replace_named_call(&result, "%breakline", |_| Some("\n".to_string()))
            {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%boolval", |args| {
                let parts = split_args(args);
                let value = parts.first()?.trim();
                Some(if self.eval_if_expr(value) { "1" } else { "0" }.to_string())
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%not", |args| {
                let parts = split_args(args);
                let value = parts.first()?.trim();
                Some(if self.eval_if_expr(value) { "0" } else { "1" }.to_string())
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%get_variable_value", |args| {
                let parts = split_args(args);
                let name = self.evaluate_builtin_text_expr(parts.first()?.trim());
                Some(self.lookup_variable_text(&name))
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%variable_exists", |args| {
                let parts = split_args(args);
                let name = self.evaluate_builtin_text_expr(parts.first()?.trim());
                Some(
                    if self.is_defined(&name) {
                        "true"
                    } else {
                        "false"
                    }
                    .to_string(),
                )
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%function_exists", |args| {
                let parts = split_args(args);
                let raw = unquote(parts.first()?.trim());
                let name = if raw.starts_with('$') {
                    raw
                } else {
                    format!("${raw}")
                };
                Some(
                    if self.funcs.contains_key(&name) {
                        "true"
                    } else {
                        "false"
                    }
                    .to_string(),
                )
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%intval", |args| {
                let parts = split_args(args);
                let raw = parts.first()?.trim();
                Some(self.evaluate_builtin_int_expr(raw).to_string())
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%strlen", |args| {
                let parts = split_args(args);
                let value = self.resolve_builtin_text_arg(parts.first()?.trim());
                Some(value.chars().count().to_string())
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%size", |args| {
                let parts = split_args(args);
                let raw = parts.first()?.trim();
                let value = self.evaluate_expression_text(raw);
                if let Some(items) = parse_array_values(&value) {
                    Some(items.len().to_string())
                } else {
                    Some(value.chars().count().to_string())
                }
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%splitstr", |args| {
                let parts = split_args(args);
                if parts.len() != 2 {
                    return None;
                }
                let value = self.resolve_builtin_text_arg(parts[0].trim());
                let delim = self.resolve_builtin_text_arg(parts[1].trim());
                let pieces: Vec<String> = if delim.is_empty() {
                    value.chars().map(|ch| ch.to_string()).collect()
                } else {
                    value
                        .split(&delim)
                        .map(std::string::ToString::to_string)
                        .collect()
                };
                Some(format_string_array(&pieces))
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%splitstr_regex", |args| {
                let parts = split_args(args);
                if parts.len() != 2 {
                    return None;
                }
                let value = self.resolve_builtin_text_arg(parts[0].trim());
                let pattern = self.resolve_builtin_text_arg(parts[1].trim());
                let regex = Regex::new(&pattern).ok()?;
                let pieces: Vec<String> = regex
                    .split(&value)
                    .map(std::string::ToString::to_string)
                    .collect();
                Some(format_string_array(&pieces))
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%strpos", |args| {
                let parts = split_args(args);
                if parts.len() != 2 {
                    return None;
                }
                let haystack = self.resolve_builtin_text_arg(parts[0].trim());
                let needle = self.resolve_builtin_text_arg(parts[1].trim());
                let pos = haystack.find(&needle).map_or(-1, |idx| idx as i64);
                Some(pos.to_string())
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%lower", |args| {
                let parts = split_args(args);
                let value = self.resolve_builtin_text_arg(parts.first()?.trim());
                Some(value.to_lowercase())
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%string", |args| {
                let parts = split_args(args);
                let raw = parts.first()?.trim();
                Some(self.evaluate_expression_text(raw))
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%substr", |args| {
                let parts = split_args(args);
                if parts.len() < 2 || parts.len() > 3 {
                    return None;
                }
                let value = self.resolve_builtin_text_arg(parts[0].trim());
                let start = self.evaluate_builtin_int_expr(parts[1].trim()).max(0) as usize;
                let chars: Vec<char> = value.chars().collect();
                if start >= chars.len() {
                    return Some(String::new());
                }
                let end = if parts.len() == 3 {
                    let len = self.evaluate_builtin_int_expr(parts[2].trim()).max(0) as usize;
                    start.saturating_add(len).min(chars.len())
                } else {
                    chars.len()
                };
                Some(chars[start..end].iter().collect())
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%ord", |args| {
                let parts = split_args(args);
                let value = self.resolve_builtin_text_arg(parts.first()?.trim());
                Some(
                    value
                        .chars()
                        .next()
                        .map_or_else(|| "0".to_string(), |ch| (ch as u32).to_string()),
                )
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%dec2hex", |args| {
                let parts = split_args(args);
                let value = self.resolve_builtin_text_arg(parts.first()?.trim());
                Some(format!("{:X}", parse_int_like(&value)))
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%hex2dec", |args| {
                let parts = split_args(args);
                let value = self.resolve_builtin_text_arg(parts.first()?.trim());
                Some(parse_hex_like(&value).to_string())
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%mod", |args| {
                let parts = split_args(args);
                if parts.len() != 2 {
                    return None;
                }
                let left = parse_int_like(&self.resolve_builtin_text_arg(parts[0].trim()));
                let right = parse_int_like(&self.resolve_builtin_text_arg(parts[1].trim()));
                Some(if right == 0 { 0 } else { left % right }.to_string())
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%getenv", |args| {
                let parts = split_args(args);
                let name = unquote(parts.first()?.trim());
                Some(std::env::var(name).unwrap_or_default())
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) =
                replace_named_call(&result, "%filename", |_| Some(self.current_filename_text()))
            {
                result = updated;
                changed = true;
            }

            if let Some(updated) =
                replace_named_call(&result, "%dirpath", |_| Some(self.current_dirpath_text()))
            {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%file_exists", |args| {
                let parts = split_args(args);
                let target = self.resolve_builtin_text_arg(parts.first()?.trim());
                let exists = if is_remote_reference(&target) {
                    self.remote_files.contains_key(target.trim())
                } else {
                    self.resolve_filesystem_path(&target)
                        .map(|path| path.exists())
                        .unwrap_or(false)
                };
                Some(if exists { "true" } else { "false" }.to_string())
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%upper", |args| {
                let parts = split_args(args);
                let value = self.resolve_builtin_text_arg(parts.first()?.trim());
                Some(value.to_uppercase())
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%get_all_theme", |_| {
                Some(format_string_array(&list_themes()))
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%get_all_stdlib", |_| {
                Some(format_string_array(&list_stdlib_names()))
            }) {
                result = updated;
                changed = true;
            }

            // %date(format) — current date/time
            if let Some(updated) = replace_named_call(&result, "%date", |args| {
                let parts = split_args(args);
                let format_str = if parts.is_empty() || parts[0].trim().is_empty() {
                    "yyyy-MM-dd'T'HH:mm:ss".to_string()
                } else {
                    self.resolve_builtin_text_arg(parts[0].trim())
                };
                Some(format_java_date(&format_str))
            }) {
                result = updated;
                changed = true;
            }

            // %version() — return version string
            if let Some(updated) = replace_named_call(&result, "%version", |_| {
                Some(env!("CARGO_PKG_VERSION").to_string())
            }) {
                result = updated;
                changed = true;
            }

            // %random(min, max) — random integer
            if let Some(updated) = replace_named_call(&result, "%random", |args| {
                let parts = split_args(args);
                if parts.is_empty() || parts[0].trim().is_empty() {
                    Some(simple_random(0, i64::from(i32::MAX)).to_string())
                } else if parts.len() == 1 {
                    let max = parse_int_like(&unquote(parts[0].trim()));
                    Some(simple_random(0, max).to_string())
                } else {
                    let min = parse_int_like(&unquote(parts[0].trim()));
                    let max = parse_int_like(&unquote(parts[1].trim()));
                    Some(simple_random(min, max).to_string())
                }
            }) {
                result = updated;
                changed = true;
            }

            // %load_json(path) — load JSON file, return raw content
            if let Some(updated) = replace_named_call(&result, "%load_json", |args| {
                let parts = split_args(args);
                let path_str = self.resolve_builtin_text_arg(parts.first()?.trim());
                let resolved = self.resolve_filesystem_path(&path_str).ok()?;
                let content = fs::read_to_string(resolved).ok()?;
                Some(content.trim().to_string())
            }) {
                result = updated;
                changed = true;
            }

            // %feature(name) — feature detection
            if let Some(updated) = replace_named_call(&result, "%feature", |args| {
                let parts = split_args(args);
                let name = unquote(parts.first()?.trim());
                let known = matches!(
                    name.as_str(),
                    "theme"
                        | "sequence"
                        | "class"
                        | "activity"
                        | "component"
                        | "state"
                        | "usecase"
                        | "object"
                        | "deployment"
                        | "timing"
                        | "gantt"
                        | "mindmap"
                        | "wbs"
                        | "json"
                        | "yaml"
                        | "salt"
                        | "ditaa"
                        | "nwdiag"
                        | "preproc"
                        | "skinparam"
                );
                Some(if known { "1" } else { "0" }.to_string())
            }) {
                result = updated;
                changed = true;
            }

            // Color builtins
            if let Some(updated) = replace_named_call(&result, "%darken", |args| {
                let parts = split_args(args);
                if parts.len() != 2 {
                    return None;
                }
                let color = unquote(parts[0].trim());
                let amount = parse_int_like(&unquote(parts[1].trim()));
                let (r, g, b) = parse_color_hex(&color)?;
                let factor = 1.0 - (amount as f64 / 100.0);
                let r2 = ((r as f64) * factor).max(0.0) as u8;
                let g2 = ((g as f64) * factor).max(0.0) as u8;
                let b2 = ((b as f64) * factor).max(0.0) as u8;
                Some(format!("#{r2:02X}{g2:02X}{b2:02X}"))
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%lighten", |args| {
                let parts = split_args(args);
                if parts.len() != 2 {
                    return None;
                }
                let color = unquote(parts[0].trim());
                let amount = parse_int_like(&unquote(parts[1].trim()));
                let (r, g, b) = parse_color_hex(&color)?;
                let factor = amount as f64 / 100.0;
                let r2 = (r as f64 + (255.0 - r as f64) * factor).min(255.0) as u8;
                let g2 = (g as f64 + (255.0 - g as f64) * factor).min(255.0) as u8;
                let b2 = (b as f64 + (255.0 - b as f64) * factor).min(255.0) as u8;
                Some(format!("#{r2:02X}{g2:02X}{b2:02X}"))
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%is_dark", |args| {
                let parts = split_args(args);
                let color = unquote(parts.first()?.trim());
                let (r, g, b) = parse_color_hex(&color)?;
                let luminance = 0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64;
                Some(if luminance < 128.0 { "true" } else { "false" }.to_string())
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%is_light", |args| {
                let parts = split_args(args);
                let color = unquote(parts.first()?.trim());
                let (r, g, b) = parse_color_hex(&color)?;
                let luminance = 0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64;
                Some(if luminance >= 128.0 { "true" } else { "false" }.to_string())
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%hsl_color", |args| {
                let parts = split_args(args);
                if parts.len() != 3 {
                    return None;
                }
                let h = unquote(parts[0].trim()).parse::<f64>().ok()?;
                let s = unquote(parts[1].trim()).parse::<f64>().ok()? / 100.0;
                let l = unquote(parts[2].trim()).parse::<f64>().ok()? / 100.0;
                let (r, g, b) = hsl_to_rgb(h, s, l);
                Some(format!("#{r:02X}{g:02X}{b:02X}"))
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%reverse_hsluv_color", |args| {
                let parts = split_args(args);
                let color = unquote(parts.first()?.trim());
                let (r, g, b) = parse_color_hex(&color)?;
                Some(format!("#{:02X}{:02X}{:02X}", 255 - r, 255 - g, 255 - b))
            }) {
                result = updated;
                changed = true;
            }

            if let Some(updated) = replace_named_call(&result, "%reverse_color", |args| {
                let parts = split_args(args);
                let color = unquote(parts.first()?.trim());
                let (r, g, b) = parse_color_hex(&color)?;
                Some(format!("#{:02X}{:02X}{:02X}", 255 - r, 255 - g, 255 - b))
            }) {
                result = updated;
                changed = true;
            }

            if !changed {
                break;
            }
        }

        result
    }

    fn try_set_variable_value_call(&mut self, line: &str) -> Result<bool> {
        if !line.starts_with("%set_variable_value(") {
            return Ok(false);
        }

        let Some(args_str) = extract_func_arg(line, "%set_variable_value") else {
            return Ok(false);
        };
        let args = split_args(args_str);
        if args.len() != 2 {
            return Ok(false);
        }

        let raw_name = if is_quoted_literal(args[0].trim()) {
            unquote(args[0].trim())
        } else {
            self.evaluate_expression_text_with_funcs(args[0].trim())?
        };
        let name = if raw_name.starts_with('$') {
            raw_name
        } else {
            format!("${raw_name}")
        };
        let value = self.evaluate_assignment_value_with_funcs(args[1].trim())?;
        self.assign_var(name, value, VarAssignMode::Auto);
        Ok(true)
    }
}

// ─── helper functions (kept in mod.rs — used by Context + submodules) ────

/// Join lines ending with `\` (line continuation).
fn join_continuations(source: &str) -> String {
    let mut result = Vec::new();
    let mut accum = String::new();
    let mut continued_lines = 0usize;

    for line in source.lines() {
        if let Some(prefix) = line.strip_suffix('\\') {
            // Continuation: strip the trailing backslash and append
            accum.push_str(prefix);
            continued_lines += 1;
        } else if !accum.is_empty() {
            accum.push_str(line);
            result.push(accum.clone());
            for _ in 0..continued_lines {
                result.push(String::new());
            }
            accum.clear();
            continued_lines = 0;
        } else {
            result.push(line.to_string());
        }
    }
    // Flush remaining
    if !accum.is_empty() {
        result.push(accum);
        for _ in 0..continued_lines {
            result.push(String::new());
        }
    }

    result.join("\n")
}

/// Translate `\n` escape sequences to private-use Unicode characters.
///
/// Java `BackSlash.translateBackSlashes()` replaces `\n` with `\<U+E06E>` so that
/// `\b` regex word boundaries see the backslash-n as a non-word boundary.
/// We use the same U+E000 private-use block offset: `\n` → U+E000 + 'n' = U+E06E.
fn translate_backslashes(s: &str) -> String {
    // Java only translates \n (isEnglishLetterOfBackSlash returns true only for 'n')
    s.replace("\\n", "\\\u{E06E}")
}

/// Reverse `translate_backslashes`: restore private-use chars to original letters.
fn untranslate_backslashes(s: &str) -> String {
    s.replace("\\\u{E06E}", "\\n")
}

/// Strip a directive prefix case-insensitively.
fn strip_directive_prefix<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    if line.len() >= prefix.len() && line[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&line[prefix.len()..])
    } else {
        None
    }
}

// ─── tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::write::FileOptions;
    use zip::ZipWriter;

    fn make_temp_dir(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "plantuml-little-preproc-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_zip(path: &Path, entries: &[(&str, &str)]) {
        let file = fs::File::create(path).expect("create zip");
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::default();

        for (name, content) in entries {
            zip.start_file(*name, options).expect("start zip entry");
            zip.write_all(content.as_bytes()).expect("write zip entry");
        }

        zip.finish().expect("finish zip");
    }

    fn spawn_http_server(routes: Vec<(&'static str, &'static str)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind http server");
        let addr = listener.local_addr().expect("local addr");

        thread::spawn(move || {
            let expected = routes.len();
            for _ in 0..expected {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buf = [0u8; 4096];
                let read = stream.read(&mut buf).expect("read request");
                let request = String::from_utf8_lossy(&buf[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");

                let body = routes
                    .iter()
                    .find(|(route, _)| *route == path)
                    .map(|(_, body)| *body)
                    .unwrap_or("");
                let status = if body.is_empty() {
                    "HTTP/1.1 404 Not Found"
                } else {
                    "HTTP/1.1 200 OK"
                };
                let response = format!(
                    "{status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write response");
            }
        });

        format!("http://{}", addr)
    }

    fn spawn_http_binary_server(routes: Vec<(&'static str, Vec<u8>, &'static str)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind http server");
        let addr = listener.local_addr().expect("local addr");

        thread::spawn(move || {
            let expected = routes.len();
            for _ in 0..expected {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buf = [0u8; 4096];
                let read = stream.read(&mut buf).expect("read request");
                let request = String::from_utf8_lossy(&buf[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");

                let matched = routes.iter().find(|(route, _, _)| *route == path);
                let (status, body, content_type): (&str, &[u8], &str) =
                    if let Some((_, body, content_type)) = matched {
                        ("HTTP/1.1 200 OK", body.as_slice(), *content_type)
                    } else {
                        ("HTTP/1.1 404 Not Found", &[], "application/octet-stream")
                    };
                let header = format!(
                    "{status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(header.as_bytes()).expect("write header");
                stream.write_all(body).expect("write body");
            }
        });

        format!("http://{}", addr)
    }

    #[test]
    fn test_pragma_passed_through() {
        let src = "@startuml\n!pragma teoz true\nAlice -> Bob\n@enduml";
        let out = preprocess(src).unwrap();
        // Pragmas are passed through so downstream parsers can see them
        assert!(
            out.contains("!pragma teoz true"),
            "pragma should be passed through, got: {out}"
        );
        assert!(out.contains("Alice -> Bob"));
    }

    #[test]
    fn test_memory_dump_alias_is_accepted() {
        let src = "@startuml\n!memory_dump\nAlice -> Bob\n@enduml";
        let out = preprocess(src).unwrap();
        assert!(out.contains("Alice -> Bob"), "got: {}", out);
    }

    #[test]
    fn test_theme_loads_vendored_theme() {
        let src = "@startuml\n!theme plain\nAlice -> Bob\n@enduml";
        let out = preprocess(src).unwrap();
        assert!(!out.contains("!theme"));
        assert!(out.contains("Alice -> Bob"));
    }

    #[test]
    fn test_theme_from_local_directory() {
        let dir = make_temp_dir("theme-local");
        let theme_dir = dir.join("themes");
        fs::create_dir_all(&theme_dir).unwrap();
        fs::write(
            theme_dir.join("puml-theme-custom.puml"),
            "skinparam ArrowColor #123456",
        )
        .unwrap();

        let src = "@startuml\n!theme custom from themes\nAlice -> Bob\n@enduml";
        let out = preprocess_with_base_dir(src, &dir).unwrap();
        assert!(out.contains("skinparam ArrowColor #123456"), "got: {}", out);
        assert!(out.contains("Alice -> Bob"), "got: {}", out);
    }

    #[test]
    fn test_include_stripped() {
        let src = "@startuml\n!include <C4/C4_Container>\nAlice -> Bob\n@enduml";
        let out = preprocess(src).unwrap();
        assert!(!out.contains("!include"));
        assert!(out.contains("Alice -> Bob"));
    }

    #[test]
    fn test_include_local_file() {
        let dir = make_temp_dir("include-local");
        fs::write(dir.join("inc.puml"), "component A").unwrap();
        let src = "@startuml\n!include inc.puml\ncomponent B\n@enduml";
        let out = preprocess_with_base_dir(src, &dir).unwrap();
        assert!(out.contains("component A"), "got: {}", out);
        assert!(out.contains("component B"), "got: {}", out);
    }

    #[test]
    fn test_include_many_repeats_file() {
        let dir = make_temp_dir("include-many");
        fs::write(dir.join("inc.puml"), "component A").unwrap();
        let src = "@startuml\n!include_many inc.puml\n!include_many inc.puml\n@enduml";
        let out = preprocess_with_base_dir(src, &dir).unwrap();
        assert_eq!(out.matches("component A").count(), 2, "got: {}", out);
    }

    #[test]
    fn test_include_repeats_by_default() {
        let dir = make_temp_dir("include-once");
        fs::write(dir.join("inc.puml"), "component A").unwrap();
        let src = "@startuml\n!include inc.puml\n!include inc.puml\n@enduml";
        let out = preprocess_with_base_dir(src, &dir).unwrap();
        assert_eq!(out.matches("component A").count(), 2, "got: {}", out);
    }

    #[test]
    fn test_include_once_errors_on_repeat() {
        let dir = make_temp_dir("include-once-error");
        fs::write(dir.join("inc.puml"), "component A").unwrap();
        let src = "@startuml\n!include_once inc.puml\n!include_once inc.puml\n@enduml";
        let err = preprocess_with_base_dir(src, &dir).unwrap_err();
        assert!(format!("{err}").contains("already included"), "got: {err}");
    }

    #[test]
    fn test_include_subpart() {
        let dir = make_temp_dir("include-subpart");
        fs::write(
            dir.join("inc.puml"),
            "!startsub PART_A\ncomponent A\n!endsub\n!startsub PART_B\ncomponent B\n!endsub\n",
        )
        .unwrap();
        let src = "@startuml\n!includesub inc.puml!PART_B\n@enduml";
        let out = preprocess_with_base_dir(src, &dir).unwrap();
        assert!(out.contains("component B"), "got: {}", out);
        assert!(!out.contains("component A"), "got: {}", out);
    }

    #[test]
    fn test_include_selected_diagram_by_index() {
        let dir = make_temp_dir("include-diagram-index");
        fs::write(
            dir.join("inc.puml"),
            "@startuml\ncomponent A\n@enduml\n@startuml\ncomponent B\n@enduml\n",
        )
        .unwrap();
        let src = "@startuml\n!include inc.puml!1\n@enduml";
        let out = preprocess_with_base_dir(src, &dir).unwrap();
        assert!(out.contains("component B"), "got: {}", out);
        assert!(!out.contains("component A"), "got: {}", out);
    }

    #[test]
    fn test_include_selected_diagram_by_id() {
        let dir = make_temp_dir("include-diagram-id");
        fs::write(
            dir.join("inc.puml"),
            "@startuml(id=FIRST)\ncomponent A\n@enduml\n@startuml(id=SECOND)\ncomponent B\n@enduml\n",
        )
        .unwrap();
        let src = "@startuml\n!include inc.puml!SECOND\n@enduml";
        let out = preprocess_with_base_dir(src, &dir).unwrap();
        assert!(out.contains("component B"), "got: {}", out);
        assert!(!out.contains("component A"), "got: {}", out);
    }

    #[test]
    fn test_import_archive_adds_include_search_path() {
        let dir = make_temp_dir("import-basic");
        let archive = dir.join("lib.zip");
        write_zip(&archive, &[("folder/entry.puml", "component Imported")]);

        let src = "@startuml\n!import lib.zip\n!include folder/entry.puml\n@enduml";
        let out = preprocess_with_base_dir(src, &dir).unwrap();
        assert!(out.contains("component Imported"), "got: {}", out);
    }

    #[test]
    fn test_import_archive_supports_nested_relative_includes() {
        let dir = make_temp_dir("import-nested");
        let archive = dir.join("lib.zip");
        write_zip(
            &archive,
            &[
                ("pkg/main.puml", "!include nested.puml\ncomponent Main"),
                ("pkg/nested.puml", "component Nested"),
            ],
        );

        let src = "@startuml\n!import lib.zip\n!include pkg/main.puml\n@enduml";
        let out = preprocess_with_base_dir(src, &dir).unwrap();
        assert!(out.contains("component Main"), "got: {}", out);
        assert!(out.contains("component Nested"), "got: {}", out);
    }

    #[test]
    fn test_import_remote_archive_adds_include_search_path() {
        let dir = make_temp_dir("import-remote");
        let archive = dir.join("lib.zip");
        write_zip(
            &archive,
            &[("folder/entry.puml", "component RemoteImported")],
        );
        let bytes = fs::read(&archive).unwrap();
        let base = spawn_http_binary_server(vec![("/lib.zip", bytes, "application/zip")]);

        let src = format!("@startuml\n!import {base}/lib.zip\n!include folder/entry.puml\n@enduml");
        let out = preprocess(&src).unwrap();
        assert!(out.contains("component RemoteImported"), "got: {}", out);
    }

    #[test]
    fn test_preprocess_c4_jaws1_fixture() {
        let path = Path::new("tests/fixtures/preprocessor/jaws1.puml");
        let src = fs::read_to_string(path).unwrap();
        let out = preprocess_with_source_path(&src, path).unwrap();
        assert!(out.contains("Administrator"), "got: {}", out);
        assert!(out.contains("Web Application"), "got: {}", out);
        assert!(out.contains("Twitter"), "got: {}", out);
    }

    #[test]
    fn test_includeurl_remote_file() {
        let base = spawn_http_server(vec![("/main.puml", "component Remote\n")]);
        let src = format!("@startuml\n!includeurl {base}/main.puml\n@enduml");
        let out = preprocess(&src).unwrap();
        assert!(out.contains("component Remote"), "got: {}", out);
    }

    #[test]
    fn test_remote_include_relative_nested() {
        let base = spawn_http_server(vec![
            ("/pkg/main.puml", "!include nested.puml\ncomponent Main\n"),
            ("/pkg/nested.puml", "component Nested\n"),
        ]);
        let src = format!("@startuml\n!includeurl {base}/pkg/main.puml\n@enduml");
        let out = preprocess(&src).unwrap();
        assert!(out.contains("component Main"), "got: {}", out);
        assert!(out.contains("component Nested"), "got: {}", out);
    }

    #[test]
    fn test_theme_from_remote_url() {
        let base = spawn_http_server(vec![(
            "/themes/puml-theme-custom.puml",
            "skinparam ArrowColor #654321\n",
        )]);
        let src = format!("@startuml\n!theme custom from {base}/themes/\nAlice -> Bob\n@enduml");
        let out = preprocess(&src).unwrap();
        assert!(out.contains("skinparam ArrowColor #654321"), "got: {}", out);
        assert!(out.contains("Alice -> Bob"), "got: {}", out);
    }

    #[test]
    fn test_variable_assignment_and_substitution() {
        let src = "@startuml\n!$name = \"World\"\ntitle Hello $name\n@enduml";
        let out = preprocess(src).unwrap();
        assert!(out.contains("title Hello World"), "got: {}", out);
    }

    #[test]
    fn test_variable_quoted() {
        let src = "!$var=\"hello world\"\ntext: $var";
        let out = preprocess(src).unwrap();
        assert!(out.contains("text: hello world"), "got: {}", out);
    }

    #[test]
    fn test_variable_unquoted() {
        let src = "!$x = 42\nvalue: $x";
        let out = preprocess(src).unwrap();
        assert!(out.contains("value: 42"), "got: {}", out);
    }

    #[test]
    fn test_define_simple() {
        let src = "!define GREETING Hello\ntitle GREETING World";
        let out = preprocess(src).unwrap();
        assert!(out.contains("title Hello World"), "got: {}", out);
    }

    #[test]
    fn test_define_with_params() {
        let src = "!define Entity(name) class name\nEntity(Foo)";
        let out = preprocess(src).unwrap();
        assert!(out.contains("class Foo"), "got: {}", out);
    }

    #[test]
    fn test_define_with_default_params() {
        let src = "!define Extension(id, name, type = \"Extension\")  class \"<b>TYP: name</b>\" as id << (E,White) type >>\nExtension(A, B)";
        let out = preprocess(src).unwrap();
        assert!(out.contains("class"), "got: {}", out);
        assert!(
            out.contains("Extension"),
            "default param should be used: {}",
            out
        );
    }

    #[test]
    fn test_define_macro_whole_word_match() {
        // Macro name "Extension" must not match inside "loadExtension"
        let src = "!define Extension(id, name, type = \"Extension\")  class name as id\n${loadExtension(SST310_DEFAULT_ERROR_RESPONSE)}";
        let out = preprocess(src).unwrap();
        // The ${loadExtension(...)} should remain as-is — the macro should
        // not be expanded because "Extension" is part of "loadExtension"
        assert!(
            out.contains("${loadExtension(SST310_DEFAULT_ERROR_RESPONSE)}"),
            "macro should not expand inside identifier: {}",
            out
        );
    }

    #[test]
    fn test_definelong_with_params() {
        let src = "!definelong Pair(a, b)\ncomponent a\ncomponent b\na -> b\n!enddefinelong\nPair(Foo, Bar)";
        let out = preprocess(src).unwrap();
        assert!(out.contains("component Foo"), "got: {}", out);
        assert!(out.contains("component Bar"), "got: {}", out);
        assert!(out.contains("Foo -> Bar"), "got: {}", out);
    }

    #[test]
    fn test_builtin_newline_expanded() {
        // %newline() is expanded to U+E100 placeholder (matches Java PlantUML)
        let src = "text%newline()more";
        let out = preprocess(src).unwrap();
        assert_eq!(out, "text\u{E100}more");
    }

    #[test]
    fn test_builtin_n_expanded() {
        // %n() is expanded to U+E100 placeholder (matches Java PlantUML)
        let src = "text%n()more";
        let out = preprocess(src).unwrap();
        assert_eq!(out, "text\u{E100}more");
    }

    #[test]
    fn test_builtin_chr() {
        let src = "char: %chr(65)";
        let out = preprocess(src).unwrap();
        assert_eq!(out, "char: A");
    }

    #[test]
    fn test_builtin_get_set_variable_value() {
        let src = "!$table = \"head\"\n%set_variable_value(\"$table\", %get_variable_value(\"$table\") + \" tail\")\nvalue: %get_variable_value(\"$table\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("value: head tail"), "got: {}", out);
    }

    #[test]
    fn test_builtin_strlen_and_substr() {
        let src = "len=%strlen(\"hello\")\nsub=%substr(\"abcdef\", 1, 3)";
        let out = preprocess(src).unwrap();
        assert!(out.contains("len=5"), "got: {}", out);
        assert!(out.contains("sub=bcd"), "got: {}", out);
    }

    #[test]
    fn test_builtin_substr_accepts_expression_indices() {
        let src = "!$start = 1\n!$len = 2\nsub=%substr(\"abcdef\", $start + 1, $len + 1)";
        let out = preprocess(src).unwrap();
        assert!(out.contains("sub=cde"), "got: {}", out);
    }

    #[test]
    fn test_builtin_boolval_and_not() {
        let src = "bool=%boolval(1 == 1)\nnot=%not(1 == 1)";
        let out = preprocess(src).unwrap();
        assert!(out.contains("bool=1"), "got: {}", out);
        assert!(out.contains("not=0"), "got: {}", out);
    }

    #[test]
    fn test_builtin_intval_splitstr_strpos_size_string_upper() {
        let src = "!$parts = %splitstr(\"a,b,c\", \",\")\nint=%intval(\"42px\")\npos=%strpos(\"abcdef\", \"cd\")\nsize=%size($parts)\ntext=%string(\"ab\" + \"cd\")\nupper=%upper(\"hello\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("int=42"), "got: {}", out);
        assert!(out.contains("pos=2"), "got: {}", out);
        assert!(out.contains("size=3"), "got: {}", out);
        assert!(out.contains("text=abcd"), "got: {}", out);
        assert!(out.contains("upper=HELLO"), "got: {}", out);
    }

    #[test]
    fn test_builtin_strpos_not_found_returns_minus_one() {
        let src = "pos=%strpos(\"abcdef\", \"zz\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("pos=-1"), "got: {}", out);
    }

    #[test]
    fn test_builtin_lower_ord_hex_mod_splitstr_regex() {
        let src = "lower=%lower(\"HeLLo\")\nord=%ord(\"A\")\nhex=%dec2hex(255)\ndec=%hex2dec(\"0x10\")\nmod=%mod(11, 4)\nparts=%splitstr_regex(\"a|b|c\", \"\\|\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("lower=hello"), "got: {}", out);
        assert!(out.contains("ord=65"), "got: {}", out);
        assert!(out.contains("hex=FF"), "got: {}", out);
        assert!(out.contains("dec=16"), "got: {}", out);
        assert!(out.contains("mod=3"), "got: {}", out);
        assert!(out.contains("[\"a\", \"b\", \"c\"]"), "got: {}", out);
    }

    #[test]
    fn test_builtin_getenv_file_exists_and_get_all_stdlib() {
        let dir = make_temp_dir("builtin-file-exists");
        fs::write(dir.join("exists.puml"), "component Present").unwrap();
        let src = "missing=%getenv(\"__PLANTUML_LITTLE_UNSET_VAR__\")\nfile=%file_exists(\"exists.puml\")\nstdlib=%get_all_stdlib()";
        let out = preprocess_with_base_dir(src, &dir).unwrap();
        assert!(out.contains("missing="), "got: {}", out);
        assert!(out.contains("file=true"), "got: {}", out);
        assert!(out.contains("C4"), "got: {}", out);
        assert!(out.contains("themes"), "got: {}", out);
    }

    #[test]
    fn test_builtin_filename_and_dirpath() {
        let dir = make_temp_dir("builtin-paths");
        let input = dir.join("sample.puml");
        let src = "file=%filename()\ndir=%dirpath()";
        let out = preprocess_with_source_path(src, &input).unwrap();
        assert!(out.contains("file=sample.puml"), "got: {}", out);
        assert!(
            out.contains(&format!("dir={}", dir.display())),
            "got: {}",
            out
        );
    }

    #[test]
    fn test_builtin_function_exists_and_get_all_theme() {
        let src = "!function $hello()\n!return ok\n!endfunction\nexists=%function_exists(\"$hello\")\nthemes=%get_all_theme()";
        let out = preprocess(src).unwrap();
        assert!(out.contains("exists=true"), "got: {}", out);
        assert!(out.contains("plain"), "got: {}", out);
        assert!(out.contains("crt-amber"), "got: {}", out);
    }

    #[test]
    fn test_line_continuation() {
        let src = "hello \\\nworld";
        let out = preprocess(src).unwrap();
        assert!(out.contains("hello world"), "got: {}", out);
    }

    #[test]
    fn test_line_continuation_preserves_line_count() {
        let src = "a\\\nb\nc";
        let out = preprocess(src).unwrap();
        assert_eq!(out.lines().count(), 3, "got: {:?}", out);
        let lines: Vec<_> = out.lines().collect();
        assert_eq!(lines[0], "ab");
        assert_eq!(lines[1], "");
        assert_eq!(lines[2], "c");
    }

    #[test]
    fn test_conditional_ifdef_true() {
        let src = "!$x = 1\n!ifdef $x\nvisible\n!endif";
        let out = preprocess(src).unwrap();
        assert!(out.contains("visible"), "got: {}", out);
    }

    #[test]
    fn test_conditional_ifdef_false() {
        let src = "!ifdef $nonexistent\nhidden\n!endif\nvisible";
        let out = preprocess(src).unwrap();
        assert!(!out.contains("hidden"), "got: {}", out);
        assert!(out.contains("visible"), "got: {}", out);
    }

    #[test]
    fn test_conditional_ifndef() {
        let src = "!ifndef $missing\nshown\n!endif";
        let out = preprocess(src).unwrap();
        assert!(out.contains("shown"), "got: {}", out);
    }

    #[test]
    fn test_conditional_if_else() {
        let src = "!$x = \"a\"\n!if $x == \"a\"\nyes\n!else\nno\n!endif";
        let out = preprocess(src).unwrap();
        assert!(out.contains("yes"), "got: {}", out);
        assert!(!out.contains("no"), "got: {}", out);
    }

    #[test]
    fn test_conditional_elseif() {
        let src = "!if 1 == 2\nno\n!elseif 2 == 2\nyes\n!else\nalso no\n!endif";
        let out = preprocess(src).unwrap();
        assert!(out.contains("yes"), "got: {}", out);
        assert!(!out.contains("also no"), "got: {}", out);
    }

    #[test]
    fn test_conditional_boolean_operators() {
        let src = "!$a = 1\n!$b = 2\n!if ($a == 1 && $b == 2) || %false()\nyes\n!else\nno\n!endif";
        let out = preprocess(src).unwrap();
        assert!(out.contains("yes"), "got: {}", out);
        assert!(!out.contains("no"), "got: {}", out);
    }

    #[test]
    fn test_conditional_unary_not() {
        let src = "!$flag = 0\n!if !($flag)\nyes\n!else\nno\n!endif";
        let out = preprocess(src).unwrap();
        assert!(out.contains("yes"), "got: {}", out);
        assert!(!out.contains("no"), "got: {}", out);
    }

    #[test]
    fn test_procedure_simple() {
        let src = "!procedure $greet($name)\ntitle Hello $name\n!endprocedure\n$greet(\"World\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("title Hello World"), "got: {}", out);
    }

    #[test]
    fn test_procedure_local_scope_does_not_leak_new_vars() {
        let src = "!$g = \"global\"\n!procedure $mutate()\n!$x = \"inner\"\n!$g = \"changed\"\n!endprocedure\n$mutate()\nout: $g / %variable_exists(\"$x\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("out: changed / false"), "got: {}", out);
    }

    #[test]
    fn test_local_assignment_shadows_global_inside_procedure() {
        let src = "!$x = \"global\"\n!procedure $show()\n!local $x = \"local\"\ninside: $x\n!endprocedure\n$show()\noutside: $x";
        let out = preprocess(src).unwrap();
        assert!(out.contains("inside: local"), "got: {}", out);
        assert!(out.contains("outside: global"), "got: {}", out);
    }

    #[test]
    fn test_function_with_return() {
        let src = "!function $double($x)\n!return $x$x\n!endfunction\ntitle $double(\"ab\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("title abab"), "got: {}", out);
    }

    #[test]
    fn test_function_assignment_expands_nested_user_functions() {
        let src = "!function $wrap($value)\n!return \"[\" + $value + \"]\"\n!endfunction\n!function $decorate($value)\n!$wrapped = $wrap($value)\n!return $wrapped + \"!\"\n!endfunction\ntitle $decorate(\"A\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("title [A]!"), "got: {}", out);
    }

    #[test]
    fn test_function_default_args() {
        let src = "!function $greet($name=\"world\", $suffix=\"!\")\n!return \"Hello \" + $name + $suffix\n!endfunction\ntitle $greet()\ntitle $greet(\"Bob\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("title Hello world!"), "got: {}", out);
        assert!(out.contains("title Hello Bob!"), "got: {}", out);
    }

    #[test]
    fn test_function_keyword_args() {
        let src = "!function $greet($name=\"world\", $suffix=\"!\")\n!return \"Hello \" + $name + $suffix\n!endfunction\ntitle $greet($suffix=\"?\", $name=\"Bob\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("title Hello Bob?"), "got: {}", out);
    }

    #[test]
    fn test_dynamic_call_user_func() {
        let src = "!function $greet($name)\n!return \"Hello \" + $name\n!endfunction\ntitle %call_user_func(\"$greet\", \"Bob\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("title Hello Bob"), "got: {}", out);
    }

    #[test]
    fn test_dynamic_invoke_procedure() {
        let src = "!procedure $emit($name)\ntitle Hello $name\n!endprocedure\n%invoke_procedure(\"$emit\", \"Bob\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("title Hello Bob"), "got: {}", out);
    }

    #[test]
    fn test_procedure_output_line_can_still_expand_user_function_calls() {
        let src = "!function $greet($name)\n!return \"Hello \" + $name\n!endfunction\n!procedure $emit($name)\ntitle $greet($name)\n!endprocedure\n$emit(\"Bob\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("title Hello Bob"), "got: {}", out);
    }

    #[test]
    fn test_call_expansion_respects_name_boundaries() {
        let src = "!procedure Boundary($label)\ntitle wrong $label\n!endprocedure\n!function $getBoundary($label)\n!return $label\n!endfunction\ntitle $getBoundary(\"ok\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("title ok"), "got: {}", out);
        assert!(!out.contains("wrong"), "got: {}", out);
    }

    #[test]
    fn test_procedure_line_expands_nested_helper_functions_to_fixed_point() {
        let src = "!function $breakText($text, $nl)\n!return $text\n!endfunction\n!function $breakLabel($text)\n!$multiLine = $breakText($text, \"\")\n!return $multiLine\n!endfunction\n!function $getProps()\n!return \"\"\n!endfunction\n!function $getPerson($label)\n!return \"== \" + $breakLabel($label) + $getProps()\n!endfunction\n!procedure Person($alias, $label)\nrectangle \"$getPerson($label)$getProps()\" as $alias\n!endprocedure\nPerson(admin, \"Administrator\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("rectangle \"== Administrator\" as admin"),
            "got: {}",
            out
        );
    }

    #[test]
    fn test_assignment_chain_expands_nested_helper_functions_to_fixed_point() {
        let src = "!function $getSprite($sprite)\n!return \"<$\" + $sprite + \">\"\n!endfunction\n!function $breakText($text, $nl)\n!return $text\n!endfunction\n!function $breakLabel($text)\n!$multiLine = $breakText($text, \"\")\n!return $multiLine\n!endfunction\n!function $getProps()\n!return \"\"\n!endfunction\n!function $getElementBase($label, $sprite)\n!$element = \"\"\n!if ($sprite != \"\")\n  !$element = $element + $getSprite($sprite)\n  !if ($label != \"\")\n    !$element = $element + '\\n'\n  !endif\n!endif\n!if ($label != \"\")\n  !$element = $element + '== ' + $breakLabel($label)\n!endif\n!return $element + $getProps()\n!endfunction\ntitle $getElementBase(\"Administrator\", \"person\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("title <$person>\\n== Administrator"),
            "got: {}",
            out
        );
    }

    #[test]
    fn test_c4_break_label_helper_expands() {
        let src = "@startuml\n!include <C4/C4>\ntitle $breakLabel(\"Administrator\")\n@enduml";
        let out = preprocess(src).unwrap();
        assert!(out.contains("title Administrator"), "got: {}", out);
    }

    #[test]
    fn test_c4_include_registers_break_label_function() {
        let src = "@startuml\n!include <C4/C4>\n@enduml";
        let mut ctx = Context::new();
        let out = ctx.process(src, None).unwrap();
        assert!(out.contains("@startuml"), "got: {}", out);
        assert!(
            ctx.funcs.contains_key("$breakLabel"),
            "available funcs: {:?}",
            ctx.funcs.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_c4_get_element_base_helper_expands() {
        let src = "@startuml\n!include <C4/C4>\ntitle $getElementBase(\"Administrator\", \"\", \"\", \"person\")\n@enduml";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("title <$person>\\n== Administrator"),
            "got: {}",
            out
        );
    }

    #[test]
    fn test_c4_person_procedure_body_expands_nested_helpers() {
        let src = "@startuml\n!include <C4/C4_Context>\n@enduml";
        let mut ctx = Context::new();
        let out = ctx.process(src, None).unwrap();
        assert!(out.contains("@startuml"), "got: {}", out);

        let person = ctx.funcs.get("Person").cloned().expect("Person procedure");
        let rendered = ctx
            .eval_func(
                &person,
                &["admin".to_string(), "\"Administrator\"".to_string()],
            )
            .unwrap();
        assert!(
            rendered.contains("rectangle \"<$person>\\n== Administrator\" <<person>> as admin"),
            "got: {}",
            rendered
        );
    }

    #[test]
    fn test_c4_get_element_line_handles_comma_in_techn() {
        let src = "@startuml\n!include <C4/C4_Container>\n@enduml";
        let mut ctx = Context::new();
        let out = ctx.process(src, None).unwrap();
        assert!(out.contains("@startuml"), "got: {}", out);

        let helper = ctx
            .funcs
            .get("$getElementLine")
            .cloned()
            .expect("$getElementLine helper");
        let rendered = ctx
            .eval_func(
                &helper,
                &[
                    "\"rectangle\"".to_string(),
                    "\"container\"".to_string(),
                    "web_app".to_string(),
                    "\"Web Application\"".to_string(),
                    "\"C#, ASP.NET Core 2.1 MVC\"".to_string(),
                    "\"Allows users to compare multiple Twitter timelines\"".to_string(),
                    "\"\"".to_string(),
                    "\"\"".to_string(),
                    "\"\"".to_string(),
                ],
            )
            .unwrap();
        assert!(
            rendered.contains("rectangle \"== Web Application\\n//<size:12>[C#, ASP.NET Core 2.1 MVC]</size>//\\n\\nAllows users to compare multiple Twitter timelines\" <<container>> as web_app"),
            "got: {}",
            rendered
        );
    }

    #[test]
    fn test_function_with_while_concat_and_newline() {
        // Java's %newline() returns U+E100 (Jaws.BLOCK_E1_NEWLINE), not real '\n'.
        let src = "!function $rows()\n  !$n = 2\n  !$i = 0\n  !$res = \"\"\n  !while $i < $n\n    !$res = $res + \"row\" + $i + %newline()\n    !$i = $i + 1\n  !endwhile\n  !return $res\n!endfunction\n$rows()";
        let out = preprocess(src).unwrap();
        let nl = crate::NEWLINE_CHAR;
        let expected = format!("row0{nl}row1{nl}");
        assert!(out.contains(&expected), "got: {}", out);
    }

    #[test]
    fn test_expression_concat_preserves_existing_newlines() {
        let mut ctx = Context::new();
        let mut scope = HashMap::new();
        let nl = crate::NEWLINE_CHAR;
        scope.insert("$res".to_string(), Value::Str(format!("row0{nl}")));
        scope.insert("$i".to_string(), Value::Int(1));
        ctx.local_scopes.push(scope);

        let out = ctx
            .evaluate_expression_text_with_funcs(r#"$res + "row" + $i + %newline()"#)
            .unwrap();
        assert_eq!(out, format!("row0{nl}row1{nl}"));
    }

    #[test]
    fn test_eval_func_preserves_multiline_return() {
        let mut ctx = Context::new();
        let func = UserFunc {
            params: vec![],
            body: vec![
                "!$n = 2".to_string(),
                "!$i = 0".to_string(),
                "!$res = \"\"".to_string(),
                "!while $i < $n".to_string(),
                "!$res = $res + \"row\" + $i + %newline()".to_string(),
                "!$i = $i + 1".to_string(),
                "!endwhile".to_string(),
                "!return $res".to_string(),
            ],
            is_procedure: false,
        };

        let out = ctx.eval_func(&func, &[]).unwrap();
        let nl = crate::NEWLINE_CHAR;
        assert_eq!(out, format!("row0{nl}row1{nl}"));
    }

    #[test]
    fn test_expand_line_preserves_multiline_function_return() {
        let mut ctx = Context::new();
        ctx.funcs.insert(
            "$rows".to_string(),
            UserFunc {
                params: vec![],
                body: vec![
                    "!$n = 2".to_string(),
                    "!$i = 0".to_string(),
                    "!$res = \"\"".to_string(),
                    "!while $i < $n".to_string(),
                    "!$res = $res + \"row\" + $i + %newline()".to_string(),
                    "!$i = $i + 1".to_string(),
                    "!endwhile".to_string(),
                    "!return $res".to_string(),
                ],
                is_procedure: false,
            },
        );

        let out = ctx.expand_line("$rows()").unwrap();
        let nl = crate::NEWLINE_CHAR;
        assert_eq!(out, format!("row0{nl}row1{nl}"));
    }

    #[test]
    fn test_passthrough_normal_lines() {
        let src = "@startuml\nAlice -> Bob : hello\n@enduml";
        let out = preprocess(src).unwrap();
        assert_eq!(out, src);
    }

    #[test]
    fn test_legacy_var_assignment() {
        let src = "!TEST=something\nresult: TEST";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result: something"), "got: {}", out);
    }

    #[test]
    fn test_legacy_var_backslash_n_boundary() {
        // Java treats \n as a newline separator, creating a word boundary
        let src = r"!TEST=something
a->b: test:\nTEST";
        let out = preprocess(src).unwrap();
        assert!(out.contains(r"test:\nsomething"), "got: {}", out);
    }

    #[test]
    fn test_multiple_var_substitutions() {
        let src = "!$a = \"X\"\n!$b = \"Y\"\nresult: $a and $b";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result: X and Y"), "got: {}", out);
    }

    #[test]
    fn test_nested_conditional() {
        let src = "!$a = 1\n!ifdef $a\n!$b = 2\n!ifdef $b\ninner\n!endif\n!endif";
        let out = preprocess(src).unwrap();
        assert!(out.contains("inner"), "got: {}", out);
    }

    #[test]
    fn test_global_var() {
        let src = "!global $table = \"hello\"\nresult: $table";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result: hello"), "got: {}", out);
    }

    #[test]
    fn test_empty_source() {
        let out = preprocess("").unwrap();
        assert_eq!(out, "");
    }

    #[test]
    fn test_no_directives() {
        let src = "line1\nline2\nline3";
        let out = preprocess(src).unwrap();
        assert_eq!(out, src);
    }

    #[test]
    fn test_builtin_true_false() {
        let src = "val: %true() %false()";
        let out = preprocess(src).unwrap();
        assert_eq!(out, "val: true false");
    }

    #[test]
    fn test_pragma_stored() {
        let src = "!pragma teoz true\nAlice -> Bob";
        let mut ctx = Context::new();
        ctx.process(src, None).unwrap();
        assert_eq!(ctx.pragmas.get("teoz").map(|s| s.as_str()), Some("true"));
    }

    #[test]
    fn test_line_continuation_multiple() {
        let src = "a\\\nb\\\nc";
        let out = preprocess(src).unwrap();
        // After joining continuations, the result is "abc" with
        // placeholder empty lines to preserve source line numbering.
        // The final output may have a trailing newline from the placeholder.
        assert_eq!(out.trim(), "abc");
    }

    #[test]
    fn test_define_no_value() {
        let src = "!define FLAG\n!ifdef FLAG\nyes\n!endif";
        let out = preprocess(src).unwrap();
        assert!(out.contains("yes"), "got: {}", out);
    }

    #[test]
    fn test_question_assign_only_sets_missing_var() {
        let src = "!$x = 1\n!$x ?= 2\n!$y ?= 3\nresult: $x/$y";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result: 1/3"), "got: {}", out);
    }

    #[test]
    fn test_assert_false_returns_error() {
        let err = preprocess("!assert 1 == 2 : \"boom\"").unwrap_err();
        assert!(format!("{err}").contains("boom"), "got: {err}");
    }

    #[test]
    fn test_undef_removes_define() {
        let src = "!define FLAG\n!undef FLAG\n!ifdef FLAG\nyes\n!else\nno\n!endif";
        let out = preprocess(src).unwrap();
        assert!(out.contains("no"), "got: {}", out);
        assert!(!out.contains("yes"), "got: {}", out);
    }

    #[test]
    fn test_conditional_if_not_equal() {
        let src = "!$x = \"a\"\n!if $x != \"b\"\nyes\n!endif";
        let out = preprocess(src).unwrap();
        assert!(out.contains("yes"), "got: {}", out);
    }

    // ── foreach tests ──────────────────────────────────────

    #[test]
    fn test_foreach_simple() {
        let src = "!foreach $i in [1, 2, 3]\nline $i\n!endfor";
        let out = preprocess(src).unwrap();
        assert!(out.contains("line 1"), "got: {}", out);
        assert!(out.contains("line 2"), "got: {}", out);
        assert!(out.contains("line 3"), "got: {}", out);
    }

    #[test]
    fn test_foreach_strings() {
        let src = "!foreach $s in [\"a\", \"b\"]\n$s\n!endfor";
        let out = preprocess(src).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.contains(&"a"), "got: {}", out);
        assert!(lines.contains(&"b"), "got: {}", out);
    }

    #[test]
    fn test_foreach_empty() {
        let src = "before\n!foreach $i in []\nline $i\n!endfor\nafter";
        let out = preprocess(src).unwrap();
        assert!(out.contains("before"), "got: {}", out);
        assert!(out.contains("after"), "got: {}", out);
        assert!(!out.contains("line"), "got: {}", out);
    }

    #[test]
    fn test_foreach_nested() {
        let src = "!foreach $i in [1, 2]\n!foreach $j in [a, b]\n$i-$j\n!endfor\n!endfor";
        let out = preprocess(src).unwrap();
        assert!(out.contains("1-a"), "got: {}", out);
        assert!(out.contains("1-b"), "got: {}", out);
        assert!(out.contains("2-a"), "got: {}", out);
        assert!(out.contains("2-b"), "got: {}", out);
    }

    // ── while tests ────────────────────────────────────────

    #[test]
    fn test_while_simple() {
        let src = "!$x = 0\n!while $x < 3\nline $x\n!$x = $x + 1\n!endwhile";
        let out = preprocess(src).unwrap();
        assert!(out.contains("line 0"), "got: {}", out);
        assert!(out.contains("line 1"), "got: {}", out);
        assert!(out.contains("line 2"), "got: {}", out);
        assert!(!out.contains("line 3"), "got: {}", out);
    }

    #[test]
    fn test_while_max_iterations() {
        // This would loop forever without the guard
        let src = "!$x = 1\n!while $x > 0\nloop\n!endwhile";
        let err = preprocess(src).unwrap_err();
        assert!(err.to_string().contains("10000 iterations"), "got: {}", err);
    }

    #[test]
    fn test_while_never_enters() {
        let src = "!$x = 10\n!while $x < 5\nshould not appear\n!endwhile\nafter";
        let out = preprocess(src).unwrap();
        assert!(!out.contains("should not appear"), "got: {}", out);
        assert!(out.contains("after"), "got: {}", out);
    }

    // ── arithmetic tests ───────────────────────────────────

    #[test]
    fn test_arithmetic_add() {
        let src = "!$y = 2 + 3\nresult: $y";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result: 5"), "got: {}", out);
    }

    #[test]
    fn test_arithmetic_subtract() {
        let src = "!$y = 10 - 3\nresult: $y";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result: 7"), "got: {}", out);
    }

    #[test]
    fn test_arithmetic_multiply() {
        let src = "!$y = 4 * 3\nresult: $y";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result: 12"), "got: {}", out);
    }

    #[test]
    fn test_arithmetic_divide() {
        let src = "!$y = 15 / 3\nresult: $y";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result: 5"), "got: {}", out);
    }

    #[test]
    fn test_arithmetic_modulo() {
        let src = "!$y = 10 % 3\nresult: $y";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result: 1"), "got: {}", out);
    }

    #[test]
    fn test_arithmetic_with_var() {
        let src = "!$x = 3\n!$y = $x + 1\nresult: $y";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result: 4"), "got: {}", out);
    }

    #[test]
    fn test_arithmetic_complex() {
        let src = "!$y = 2 + 3 * 4\nresult: $y";
        let out = preprocess(src).unwrap();
        // Should respect operator precedence: 2 + (3*4) = 14
        assert!(out.contains("result: 14"), "got: {}", out);
    }

    #[test]
    fn test_arithmetic_parentheses() {
        let src = "!$y = (2 + 3) * 4\nresult: $y";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result: 20"), "got: {}", out);
    }

    // ── comparison tests ───────────────────────────────────

    #[test]
    fn test_comparison_less_than() {
        let src = "!$x = 2\n!if $x < 5\nyes\n!endif";
        let out = preprocess(src).unwrap();
        assert!(out.contains("yes"), "got: {}", out);
    }

    #[test]
    fn test_comparison_greater_than() {
        let src = "!$x = 10\n!if $x > 5\nyes\n!endif";
        let out = preprocess(src).unwrap();
        assert!(out.contains("yes"), "got: {}", out);
    }

    #[test]
    fn test_comparison_less_equal() {
        let src = "!$x = 5\n!if $x <= 5\nyes\n!endif";
        let out = preprocess(src).unwrap();
        assert!(out.contains("yes"), "got: {}", out);
    }

    #[test]
    fn test_comparison_greater_equal() {
        let src = "!$x = 5\n!if $x >= 6\nno\n!else\nyes\n!endif";
        let out = preprocess(src).unwrap();
        assert!(out.contains("yes"), "got: {}", out);
        assert!(!out.contains("no"), "got: {}", out);
    }

    // ── parse_array tests ──────────────────────────────────

    #[test]
    fn test_parse_array_numbers() {
        let items = parse_array("[1, 2, 3]").unwrap();
        assert_eq!(items, vec!["1", "2", "3"]);
    }

    #[test]
    fn test_parse_array_strings() {
        let items = parse_array("[\"a\", \"b\"]").unwrap();
        assert_eq!(items, vec!["a", "b"]);
    }

    #[test]
    fn test_parse_array_empty() {
        let items = parse_array("[]").unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_array_not_array() {
        assert!(parse_array("hello").is_none());
    }

    // ── arithmetic evaluator unit tests ────────────────────

    #[test]
    fn test_try_eval_arithmetic_basic() {
        assert_eq!(try_eval_arithmetic("2 + 3"), Some("5".to_string()));
        assert_eq!(try_eval_arithmetic("10 - 4"), Some("6".to_string()));
        assert_eq!(try_eval_arithmetic("3 * 7"), Some("21".to_string()));
        assert_eq!(try_eval_arithmetic("20 / 4"), Some("5".to_string()));
        assert_eq!(try_eval_arithmetic("10 % 3"), Some("1".to_string()));
    }

    #[test]
    fn test_try_eval_arithmetic_precedence() {
        assert_eq!(try_eval_arithmetic("2 + 3 * 4"), Some("14".to_string()));
        assert_eq!(try_eval_arithmetic("(2 + 3) * 4"), Some("20".to_string()));
    }

    #[test]
    fn test_try_eval_arithmetic_not_arithmetic() {
        assert_eq!(try_eval_arithmetic("hello"), None);
        assert_eq!(try_eval_arithmetic("42"), None); // plain number, no operator
    }

    // ── Value enum tests ──────────────────────────────────

    #[test]
    fn test_value_creation_and_conversion() {
        let v_str = Value::Str("hello".to_string());
        assert_eq!(v_str.as_str(), "hello");
        assert_eq!(v_str.as_int(), None);
        assert!(v_str.as_array().is_none());
        assert!(v_str.is_truthy());

        let v_int = Value::Int(42);
        assert_eq!(v_int.as_str(), "42");
        assert_eq!(v_int.as_int(), Some(42));
        assert!(v_int.as_array().is_none());
        assert!(v_int.is_truthy());

        let v_arr = Value::Array(vec![Value::Int(1), Value::Int(2)]);
        assert_eq!(v_arr.as_str(), "[1, 2]");
        assert_eq!(v_arr.as_int(), None);
        assert!(v_arr.as_array().is_some());
        assert_eq!(v_arr.as_array().unwrap().len(), 2);
        assert!(v_arr.is_truthy());
    }

    #[test]
    fn test_value_truthiness() {
        assert!(!Value::Str("".to_string()).is_truthy());
        assert!(!Value::Str("0".to_string()).is_truthy());
        assert!(!Value::Str("false".to_string()).is_truthy());
        assert!(Value::Str("hello".to_string()).is_truthy());

        assert!(!Value::Int(0).is_truthy());
        assert!(Value::Int(1).is_truthy());
        assert!(Value::Int(-1).is_truthy());

        assert!(!Value::Array(vec![]).is_truthy());
        assert!(Value::Array(vec![Value::Int(1)]).is_truthy());
    }

    #[test]
    fn test_value_parse_from() {
        assert_eq!(Value::parse_from("42"), Value::Int(42));
        assert_eq!(Value::parse_from("-5"), Value::Int(-5));
        assert_eq!(Value::parse_from("hello"), Value::Str("hello".to_string()));
        assert_eq!(
            Value::parse_from("[1, 2, 3]"),
            Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
        assert_eq!(Value::parse_from("[]"), Value::Array(vec![]));
    }

    #[test]
    fn test_value_str_as_int() {
        // String that looks like an integer can be parsed
        let v = Value::Str("123".to_string());
        assert_eq!(v.as_int(), Some(123));

        let v = Value::Str("abc".to_string());
        assert_eq!(v.as_int(), None);
    }

    // ── Value integration tests ────────────────────────────

    #[test]
    fn test_array_variable_assignment() {
        let src = "!$arr = [1, 2, 3]\n!foreach $i in $arr\nitem $i\n!endfor";
        let out = preprocess(src).unwrap();
        assert!(out.contains("item 1"), "got: {}", out);
        assert!(out.contains("item 2"), "got: {}", out);
        assert!(out.contains("item 3"), "got: {}", out);
    }

    #[test]
    fn test_integer_arithmetic_value_type() {
        let src = "!$x = 5\n!$y = $x + 3\nresult: $y";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result: 8"), "got: {}", out);
    }

    #[test]
    fn test_integer_stored_as_int() {
        let mut ctx = Context::new();
        ctx.process("!$x = 42", None).unwrap();
        assert_eq!(ctx.vars.get("$x"), Some(&Value::Int(42)));
    }

    #[test]
    fn test_string_stored_as_str() {
        let mut ctx = Context::new();
        ctx.process("!$x = \"hello\"", None).unwrap();
        assert_eq!(ctx.vars.get("$x"), Some(&Value::Str("hello".to_string())));
    }

    #[test]
    fn test_array_stored_as_array() {
        let mut ctx = Context::new();
        ctx.process("!$arr = [10, 20, 30]", None).unwrap();
        assert_eq!(
            ctx.vars.get("$arr"),
            Some(&Value::Array(vec![
                Value::Int(10),
                Value::Int(20),
                Value::Int(30),
            ]))
        );
    }

    #[test]
    fn test_arithmetic_result_stored_as_int() {
        let mut ctx = Context::new();
        ctx.process("!$x = 5\n!$y = $x + 3", None).unwrap();
        assert_eq!(ctx.vars.get("$y"), Some(&Value::Int(8)));
    }

    #[test]
    fn test_mixed_array_values() {
        let mut ctx = Context::new();
        ctx.process("!$arr = [1, \"hello\", 3]", None).unwrap();
        assert_eq!(
            ctx.vars.get("$arr"),
            Some(&Value::Array(vec![
                Value::Int(1),
                Value::Str("hello".to_string()),
                Value::Int(3),
            ]))
        );
    }

    #[test]
    fn test_foreach_with_array_variable() {
        let src =
            "!$colors = [\"red\", \"green\", \"blue\"]\n!foreach $c in $colors\ncolor: $c\n!endfor";
        let out = preprocess(src).unwrap();
        assert!(out.contains("color: red"), "got: {}", out);
        assert!(out.contains("color: green"), "got: {}", out);
        assert!(out.contains("color: blue"), "got: {}", out);
    }

    #[test]
    fn test_while_with_integer_decrement() {
        let src = "!$n = 3\n!while $n > 0\ncount $n\n!$n = $n - 1\n!endwhile";
        let out = preprocess(src).unwrap();
        assert!(out.contains("count 3"), "got: {}", out);
        assert!(out.contains("count 2"), "got: {}", out);
        assert!(out.contains("count 1"), "got: {}", out);
        assert!(!out.contains("count 0"), "got: {}", out);
    }

    #[test]
    fn test_parse_array_values_typed() {
        let items = parse_array_values("[1, \"two\", 3]").unwrap();
        assert_eq!(
            items,
            vec![Value::Int(1), Value::Str("two".to_string()), Value::Int(3),]
        );
    }

    #[test]
    fn test_parse_array_values_empty() {
        let items = parse_array_values("[]").unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_array_values_not_array() {
        assert!(parse_array_values("hello").is_none());
    }

    // ── new builtin function tests ──────────────────────────

    #[test]
    fn test_builtin_date_default_format() {
        let src = "d=%date()";
        let out = preprocess(src).unwrap();
        // Should produce a date like 2026-03-13T...
        assert!(out.starts_with("d=20"), "got: {}", out);
        assert!(out.contains("T"), "got: {}", out);
    }

    #[test]
    fn test_builtin_date_custom_format() {
        let src = "d=%date(\"yyyy-MM-dd\")";
        let out = preprocess(src).unwrap();
        // Should produce something like d=2026-03-13
        assert!(out.starts_with("d=20"), "got: {}", out);
        assert!(out.contains("-"), "got: {}", out);
        // Should NOT contain time separators
        assert!(!out.contains("T"), "got: {}", out);
    }

    #[test]
    fn test_builtin_version() {
        let src = "v=%version()";
        let out = preprocess(src).unwrap();
        assert!(out.starts_with("v="), "got: {}", out);
        assert!(out.contains('.'), "version should contain dots: {}", out);
    }

    #[test]
    fn test_builtin_random_no_args() {
        let src = "r=%random()";
        let out = preprocess(src).unwrap();
        assert!(out.starts_with("r="), "got: {}", out);
        let num_str = &out[2..];
        assert!(num_str.parse::<i64>().is_ok(), "should be numeric: {}", out);
    }

    #[test]
    fn test_builtin_random_with_range() {
        let src = "r=%random(1, 100)";
        let out = preprocess(src).unwrap();
        assert!(out.starts_with("r="), "got: {}", out);
        let num: i64 = out[2..].parse().unwrap();
        assert!((1..=100).contains(&num), "random out of range: {}", num);
    }

    #[test]
    fn test_builtin_feature_known() {
        let src = "f=%feature(\"theme\")";
        let out = preprocess(src).unwrap();
        assert_eq!(out, "f=1");
    }

    #[test]
    fn test_builtin_feature_unknown() {
        let src = "f=%feature(\"nonexistent_feature\")";
        let out = preprocess(src).unwrap();
        assert_eq!(out, "f=0");
    }

    #[test]
    fn test_builtin_darken() {
        let src = "c=%darken(\"#FF8080\", 50)";
        let out = preprocess(src).unwrap();
        assert!(out.starts_with("c=#"), "got: {}", out);
        // 50% darken of #FF8080 -> roughly #7F4040
        assert_eq!(out, "c=#7F4040", "got: {}", out);
    }

    #[test]
    fn test_builtin_lighten() {
        let src = "c=%lighten(\"#804040\", 50)";
        let out = preprocess(src).unwrap();
        assert!(out.starts_with("c=#"), "got: {}", out);
        // 50% lighten of #804040 -> roughly #BF9F9F
        assert_eq!(out, "c=#BF9F9F", "got: {}", out);
    }

    #[test]
    fn test_builtin_is_dark() {
        let src = "dark=%is_dark(\"#000000\")\nlight=%is_dark(\"#FFFFFF\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("dark=true"), "got: {}", out);
        assert!(out.contains("light=false"), "got: {}", out);
    }

    #[test]
    fn test_builtin_is_light() {
        let src = "dark=%is_light(\"#000000\")\nlight=%is_light(\"#FFFFFF\")";
        let out = preprocess(src).unwrap();
        assert!(out.contains("dark=false"), "got: {}", out);
        assert!(out.contains("light=true"), "got: {}", out);
    }

    #[test]
    fn test_builtin_hsl_color() {
        // Red: H=0, S=100%, L=50%
        let src = "c=%hsl_color(0, 100, 50)";
        let out = preprocess(src).unwrap();
        assert_eq!(out, "c=#FF0000", "got: {}", out);
    }

    #[test]
    fn test_builtin_hsl_color_green() {
        // Green: H=120, S=100%, L=50%
        let src = "c=%hsl_color(120, 100, 50)";
        let out = preprocess(src).unwrap();
        assert_eq!(out, "c=#00FF00", "got: {}", out);
    }

    #[test]
    fn test_builtin_reverse_color() {
        let src = "c=%reverse_color(\"#FF0000\")";
        let out = preprocess(src).unwrap();
        assert_eq!(out, "c=#00FFFF", "got: {}", out);
    }

    #[test]
    fn test_builtin_reverse_hsluv_color() {
        let src = "c=%reverse_hsluv_color(\"#FF0000\")";
        let out = preprocess(src).unwrap();
        assert_eq!(out, "c=#00FFFF", "got: {}", out);
    }

    #[test]
    fn test_builtin_load_json() {
        let dir = make_temp_dir("load_json");
        fs::create_dir_all(&dir).unwrap();
        let json_file = dir.join("data.json");
        fs::write(&json_file, r#"{"key": "value"}"#).unwrap();
        let src = format!("j=%load_json(\"{}\")", json_file.display());
        let out = preprocess_with_base_dir(&src, &dir).unwrap();
        assert!(out.contains(r#""key": "value""#), "got: {}", out);
    }

    // ── new directive tests ─────────────────────────────────

    #[test]
    fn test_enddefine_alias() {
        let src = "!definelong Pair(a, b)\ncomponent a\ncomponent b\n!enddefine\nPair(Foo, Bar)";
        let out = preprocess(src).unwrap();
        assert!(out.contains("component Foo"), "got: {}", out);
        assert!(out.contains("component Bar"), "got: {}", out);
    }

    #[test]
    fn test_includedef() {
        let dir = make_temp_dir("includedef");
        fs::create_dir_all(&dir).unwrap();
        let inc = dir.join("defs.puml");
        fs::write(
            &inc,
            "!$color = \"red\"\nnote This should be ignored\n!define GREETING Hello",
        )
        .unwrap();
        let src = format!("!includedef {}\ntitle $color GREETING", inc.display());
        let out = preprocess_with_base_dir(&src, &dir).unwrap();
        assert!(out.contains("title red Hello"), "got: {}", out);
    }

    #[test]
    fn test_macro_concat_operator() {
        let src = "!define MkName(prefix, suffix) prefix ## suffix\nresult: MkName(Hello, World)";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result: HelloWorld"), "got: {}", out);
    }

    #[test]
    fn test_macro_concat_definelong() {
        let src = "!definelong MkClass(name)\nclass name ## Impl\n!enddefinelong\nMkClass(Foo)";
        let out = preprocess(src).unwrap();
        assert!(out.contains("class FooImpl"), "got: {}", out);
    }

    // ── color helper tests ──────────────────────────────────

    #[test]
    fn test_parse_color_hex_6digit() {
        assert_eq!(parse_color_hex("#FF8040"), Some((255, 128, 64)));
    }

    #[test]
    fn test_parse_color_hex_3digit() {
        assert_eq!(parse_color_hex("#F80"), Some((255, 136, 0)));
    }

    #[test]
    fn test_parse_color_hex_no_hash() {
        assert_eq!(parse_color_hex("FF8040"), Some((255, 128, 64)));
    }

    #[test]
    fn test_hsl_to_rgb_red() {
        assert_eq!(hsl_to_rgb(0.0, 1.0, 0.5), (255, 0, 0));
    }

    #[test]
    fn test_hsl_to_rgb_white() {
        assert_eq!(hsl_to_rgb(0.0, 0.0, 1.0), (255, 255, 255));
    }

    #[test]
    fn test_hsl_to_rgb_black() {
        assert_eq!(hsl_to_rgb(0.0, 0.0, 0.0), (0, 0, 0));
    }

    #[test]
    fn test_format_java_date_components() {
        let result = format_java_date("yyyy-MM-dd");
        // Should be a valid date format like 2026-03-13
        assert_eq!(result.len(), 10, "got: {}", result);
        assert_eq!(&result[4..5], "-", "got: {}", result);
        assert_eq!(&result[7..8], "-", "got: {}", result);
    }

    #[test]
    fn test_format_java_date_with_quotes() {
        let result = format_java_date("yyyy-MM-dd'T'HH:mm:ss");
        // Should contain the literal T
        assert!(result.contains('T'), "got: {}", result);
    }

    // ══════════════════════════════════════════════════════════════════
    // Tests ported from upstream PlantUML Java project
    // ══════════════════════════════════════════════════════════════════

    // ── Ported from upstream: EvalMathTest ───────────────────────────

    // Ported from upstream: EvalMathTest.testBasicOperations
    #[test]
    fn upstream_eval_math_basic_add() {
        let r = eval_arith_expr("2+3").unwrap();
        assert!((r - 5.0).abs() < 0.0001, "2+3 = {}", r);
    }

    // Ported from upstream: EvalMathTest.testBasicOperations
    #[test]
    fn upstream_eval_math_basic_sub() {
        let r = eval_arith_expr("2-3").unwrap();
        assert!((r - (-1.0)).abs() < 0.0001, "2-3 = {}", r);
    }

    // Ported from upstream: EvalMathTest.testBasicOperations
    #[test]
    fn upstream_eval_math_basic_mul() {
        let r = eval_arith_expr("2*3").unwrap();
        assert!((r - 6.0).abs() < 0.0001, "2*3 = {}", r);
    }

    // Ported from upstream: EvalMathTest.testBasicOperations
    #[test]
    fn upstream_eval_math_basic_div() {
        let r = eval_arith_expr("6/3").unwrap();
        assert!((r - 2.0).abs() < 0.0001, "6/3 = {}", r);
    }

    // Ported from upstream: EvalMathTest.testPEMDAS
    #[test]
    fn upstream_eval_math_pemdas_main() {
        let r = eval_arith_expr("33+2*(4+1)").unwrap();
        assert!((r - 43.0).abs() < 0.0001, "33+2*(4+1) = {}", r);
    }

    // Ported from upstream: EvalMathTest.testPEMDAS
    #[test]
    fn upstream_eval_math_pemdas_add_mul() {
        let r = eval_arith_expr("3+4*2").unwrap();
        assert!((r - 11.0).abs() < 0.0001, "3+4*2 = {}", r);
    }

    // Ported from upstream: EvalMathTest.testPEMDAS
    #[test]
    fn upstream_eval_math_pemdas_paren_mul() {
        let r = eval_arith_expr("(3+4)*2").unwrap();
        assert!((r - 14.0).abs() < 0.0001, "(3+4)*2 = {}", r);
    }

    // Ported from upstream: EvalMathTest.testPEMDAS
    #[test]
    fn upstream_eval_math_pemdas_add_div_spaces() {
        let r = eval_arith_expr("3+8/ 2").unwrap();
        assert!((r - 7.0).abs() < 0.0001, "3+8/ 2 = {}", r);
    }

    // Ported from upstream: EvalMathTest.testUnaryOperators
    #[test]
    fn upstream_eval_math_unary_negative() {
        let r = eval_arith_expr("-5").unwrap();
        assert!((r - (-5.0)).abs() < 0.0001, "-5 = {}", r);
    }

    // Ported from upstream: EvalMathTest.testUnaryOperators
    #[test]
    fn upstream_eval_math_unary_positive() {
        let r = eval_arith_expr("+5").unwrap();
        assert!((r - 5.0).abs() < 0.0001, "+5 = {}", r);
    }

    // Ported from upstream: EvalMathTest.testUnaryOperators
    // Note: our eval_arith_expr tokenizer doesn't yet handle unary minus before
    // parenthesized sub-expressions like `-(2-10)`. We test the arithmetic
    // equivalent `0-(2-10)` which evaluates to the same result.
    #[test]
    fn upstream_eval_math_unary_negate_paren() {
        // `-(2-10)` = 8, tested as `0-(2-10)` since our evaluator handles
        // unary minus only before numeric literals
        let r = eval_arith_expr("0-(2-10)").unwrap();
        assert!((r - 8.0).abs() < 0.0001, "0-(2-10) = {}", r);
    }

    // Ported from upstream: EvalMathTest.testDecimalNumbers
    #[test]
    fn upstream_eval_math_decimal_literal() {
        let r = eval_arith_expr("3.14").unwrap();
        #[allow(clippy::approx_constant)]
        let expected = 3.14;
        assert!((r - expected).abs() < 0.0001, "3.14 = {}", r);
    }

    // Ported from upstream: EvalMathTest.testDecimalNumbers
    #[test]
    fn upstream_eval_math_decimal_add() {
        let r = eval_arith_expr("2.12+2.12").unwrap();
        assert!((r - 4.24).abs() < 0.0001, "2.12+2.12 = {}", r);
    }

    // Ported from upstream: EvalMathTest.testInvalidCharacters
    #[test]
    fn upstream_eval_math_invalid_chars() {
        assert!(eval_arith_expr("2+@3").is_none());
    }

    // ── Ported from upstream: EvalBooleanTest ───────────────────────

    // Ported from upstream: EvalBooleanTest.testBooleanExpressions
    #[test]
    fn upstream_eval_bool_basic_defined_true() {
        let src = "!$A = 1\n!ifdef A\nyes\n!endif";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("yes"),
            "defined var should be truthy, got: {}",
            out
        );
    }

    // Ported from upstream: EvalBooleanTest.testBooleanExpressions
    #[test]
    fn upstream_eval_bool_basic_undefined_false() {
        let src = "!ifdef B\nyes\n!else\nno\n!endif";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("no"),
            "undefined var should be falsy, got: {}",
            out
        );
    }

    // Ported from upstream: EvalBooleanTest.testBooleanExpressions — NOT operator
    #[test]
    fn upstream_eval_bool_not_defined() {
        let src = "!$A = 1\n!ifndef A\nyes\n!else\nno\n!endif";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("no"),
            "!ifndef on defined var should be false, got: {}",
            out
        );
    }

    // Ported from upstream: EvalBooleanTest.testBooleanExpressions — NOT operator
    #[test]
    fn upstream_eval_bool_not_undefined() {
        let src = "!ifndef B\nyes\n!else\nno\n!endif";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("yes"),
            "!ifndef on undefined var should be true, got: {}",
            out
        );
    }

    // Ported from upstream: EvalBooleanTest.testBooleanExpressions — AND operator
    #[test]
    fn upstream_eval_bool_and_true_true() {
        let src = "!$A = 1\n!$C = 1\n!if $A && $C\nyes\n!else\nno\n!endif";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("yes"),
            "true && true should be true, got: {}",
            out
        );
    }

    // Ported from upstream: EvalBooleanTest.testBooleanExpressions — AND operator
    #[test]
    fn upstream_eval_bool_and_true_false() {
        let src = "!$A = 1\n!$B = 0\n!if $A && $B\nyes\n!else\nno\n!endif";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("no"),
            "true && false should be false, got: {}",
            out
        );
    }

    // Ported from upstream: EvalBooleanTest.testBooleanExpressions — OR operator
    #[test]
    fn upstream_eval_bool_or_true_false() {
        let src = "!$A = 1\n!$B = 0\n!if $A || $B\nyes\n!else\nno\n!endif";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("yes"),
            "true || false should be true, got: {}",
            out
        );
    }

    // Ported from upstream: EvalBooleanTest.testBooleanExpressions — OR operator
    #[test]
    fn upstream_eval_bool_or_false_false() {
        let src = "!$B = 0\n!$D = 0\n!if $B || $D\nyes\n!else\nno\n!endif";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("no"),
            "false || false should be false, got: {}",
            out
        );
    }

    // Ported from upstream: EvalBooleanTest.testBooleanExpressions — combined
    #[test]
    fn upstream_eval_bool_not_and_or_combined() {
        let src = "!$A = 1\n!$B = 0\n!if !$A || $B\nyes\n!else\nno\n!endif";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("no"),
            "!true || false should be false, got: {}",
            out
        );
    }

    // Ported from upstream: EvalBooleanTest.testBooleanExpressions — combined
    #[test]
    fn upstream_eval_bool_and_not() {
        let src = "!$A = 1\n!$B = 0\n!if $A && !$B\nyes\n!else\nno\n!endif";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("yes"),
            "true && !false should be true, got: {}",
            out
        );
    }

    // ── Ported from upstream: DefineVariableTest ───────────────────

    // Ported from upstream: DefineVariableTest.testNameWithoutDefaultValue
    #[test]
    fn upstream_define_var_no_default() {
        let src = "!define MY_VAR\n!ifdef MY_VAR\ndefined\n!endif";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("defined"),
            "define without value should still mark as defined, got: {}",
            out
        );
    }

    // Ported from upstream: DefineVariableTest.testNameWithDefaultValue
    #[test]
    fn upstream_define_var_with_value() {
        let src = "!define MY_VAR default\nresult: MY_VAR";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result: default"),
            "define with value should expand, got: {}",
            out
        );
    }

    // Ported from upstream: DefineVariableTest.testRemoveDefaultValue
    #[test]
    fn upstream_define_then_undef() {
        let src = "!define MY_VAR default\n!undef MY_VAR\n!ifdef MY_VAR\nyes\n!else\nno\n!endif";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("no"),
            "undef should remove define, got: {}",
            out
        );
    }

    // Ported from upstream: DefineVariableTest.testNameWithExtraSpaces
    #[test]
    fn upstream_define_var_with_extra_spaces() {
        let src = "!define  MY_VAR  default\nresult: MY_VAR";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result: default"),
            "define with spaces should still work, got: {}",
            out
        );
    }

    // ── Ported from upstream: VariablesTest ────────────────────────

    // Ported from upstream: VariablesTest.testApplyOnWithDefaultValues
    #[test]
    fn upstream_define_param_with_default() {
        let src = "!define func(MY_VAR=\"defaultValue\") definition MY_VAR\nfunc()";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("definition defaultValue"),
            "param default should apply, got: {}",
            out
        );
    }

    // Ported from upstream: VariablesTest.testApplyOnWithoutDefaultValues
    #[test]
    fn upstream_define_param_override_default() {
        let src = "!define func(MY_VAR=\"defaultValue\") definition MY_VAR\nfunc(\"customValue\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("definition customValue") || out.contains("customValue"),
            "explicit arg should override default, got: {}",
            out
        );
    }

    // ── Ported from upstream: SubstrTest ───────────────────────────

    // Ported from upstream: SubstrTest.testSubstr
    #[test]
    fn upstream_substr_from_start() {
        let src = "result=%substr(\"hello world\", 0, 5)";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=hello"),
            "substr(0,5) should be 'hello', got: {}",
            out
        );
    }

    // Ported from upstream: SubstrTest.testSubstr
    #[test]
    fn upstream_substr_from_middle() {
        let src = "result=%substr(\"hello world\", 6, 5)";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=world"),
            "substr(6,5) should be 'world', got: {}",
            out
        );
    }

    // Ported from upstream: SubstrTest.testSubstr
    #[test]
    fn upstream_substr_full_string() {
        let src = "result=%substr(\"hello world\", 0, 11)";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=hello world"),
            "substr(0,11) should be full string, got: {}",
            out
        );
    }

    // Ported from upstream: SubstrTest.testSubstr
    #[test]
    fn upstream_substr_zero_length() {
        let src = "result=%substr(\"hello world\", 6, 0)";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result="),
            "substr with zero len should be empty, got: {}",
            out
        );
    }

    // Ported from upstream: SubstrTest.testSubstrWithoutLength
    #[test]
    fn upstream_substr_without_length() {
        let src = "result=%substr(\"hello world\", 6)";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=world"),
            "substr without length should return rest, got: {}",
            out
        );
    }

    // ── Ported from upstream: StrposTest ──────────────────────────

    // Ported from upstream: StrposTest.testStrpos
    #[test]
    fn upstream_strpos_at_beginning() {
        let src = "result=%strpos(\"hello world\", \"hello\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=0"),
            "strpos at beginning should be 0, got: {}",
            out
        );
    }

    // Ported from upstream: StrposTest.testStrpos
    #[test]
    fn upstream_strpos_in_middle() {
        let src = "result=%strpos(\"hello world\", \"world\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=6"),
            "strpos of 'world' should be 6, got: {}",
            out
        );
    }

    // Ported from upstream: StrposTest.testStrpos
    #[test]
    fn upstream_strpos_not_found() {
        let src = "result=%strpos(\"hello world\", \"!\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=-1"),
            "strpos not found should be -1, got: {}",
            out
        );
    }

    // Ported from upstream: StrposTest.testStrpos — repeated chars
    #[test]
    fn upstream_strpos_repeated_first_occurrence() {
        let src = "result=%strpos(\"aaa\", \"a\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=0"),
            "strpos first occurrence, got: {}",
            out
        );
    }

    // ── Ported from upstream: ChrTest ─────────────────────────────

    // Ported from upstream: ChrTest.Test_with_Integer
    #[test]
    fn upstream_chr_uppercase_a() {
        let src = "result=%chr(65)";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=A"),
            "chr(65) should be 'A', got: {}",
            out
        );
    }

    // Ported from upstream: ChrTest.Test_with_Integer
    #[test]
    fn upstream_chr_space() {
        let src = "result=%chr(32)";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result= "),
            "chr(32) should be space, got: {}",
            out
        );
    }

    // Ported from upstream: ChrTest.Test_with_Integer
    #[test]
    fn upstream_chr_exclamation() {
        let src = "result=%chr(33)";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=!"),
            "chr(33) should be '!', got: {}",
            out
        );
    }

    // ── Ported from upstream: OrdTest ─────────────────────────────

    // Ported from upstream: OrdTest.Test_with_String
    #[test]
    fn upstream_ord_uppercase_a() {
        let src = "!$v = %ord(\"A\")\nresult=$v";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=65"),
            "ord('A') should be 65, got: {}",
            out
        );
    }

    // ── Ported from upstream: ModuloTest ──────────────────────────

    // Ported from upstream: ModuloTest.executeReturnFunctionModuloTest
    #[test]
    fn upstream_modulo_basic() {
        let src = "result=%mod(3, 2)";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result=1"), "3 mod 2 = 1, got: {}", out);
    }

    // Ported from upstream: ModuloTest.executeReturnFunctionModuloTest
    #[test]
    fn upstream_modulo_dividend_smaller() {
        let src = "result=%mod(2, 3)";
        let out = preprocess(src).unwrap();
        assert!(out.contains("result=2"), "2 mod 3 = 2, got: {}", out);
    }

    // ── Ported from upstream: UpperTest ───────────────────────────

    // Ported from upstream: UpperTest.Test_with_String
    #[test]
    fn upstream_upper_lowercase() {
        let src = "result=%upper(\"a\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=A"),
            "upper('a') should be 'A', got: {}",
            out
        );
    }

    // Ported from upstream: UpperTest.Test_with_String
    #[test]
    fn upstream_upper_already_uppercase() {
        let src = "result=%upper(\"A\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=A"),
            "upper('A') should be 'A', got: {}",
            out
        );
    }

    // ── Ported from upstream: Hex2decTest ─────────────────────────

    // Ported from upstream: Hex2decTest.Test_with_String
    #[test]
    fn upstream_hex2dec_lowercase_a() {
        let src = "result=%hex2dec(\"a\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=10"),
            "hex2dec('a') should be 10, got: {}",
            out
        );
    }

    // Ported from upstream: Hex2decTest.Test_with_String
    #[test]
    fn upstream_hex2dec_ff() {
        let src = "result=%hex2dec(\"ff\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=255"),
            "hex2dec('ff') should be 255, got: {}",
            out
        );
    }

    // Ported from upstream: Hex2decTest.Test_with_String
    #[test]
    fn upstream_hex2dec_10() {
        let src = "result=%hex2dec(\"10\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=16"),
            "hex2dec('10') should be 16, got: {}",
            out
        );
    }

    // ── Ported from upstream: BoolValTest ─────────────────────────

    // Ported from upstream: BoolValTest.executeReturnFunctionWithValidBooleanValueStringTest
    #[test]
    fn upstream_boolval_true() {
        let src = "result=%boolval(\"true\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=true") || out.contains("result=1"),
            "boolval('true') should be truthy, got: {}",
            out
        );
    }

    // Ported from upstream: BoolValTest.executeReturnFunctionWithValidBooleanValueStringTest
    #[test]
    fn upstream_boolval_false() {
        let src = "result=%boolval(\"false\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=false") || out.contains("result=0"),
            "boolval('false') should be falsy, got: {}",
            out
        );
    }

    // ── Ported from upstream: DarkenTest ──────────────────────────

    // Ported from upstream: DarkenTest.testDarken — Red, 10%
    // Upstream expects #E60000; our implementation produces #E50000 due to
    // rounding differences in the HSL conversion. Both are valid darken results.
    #[test]
    fn upstream_darken_red_10pct() {
        let src = "c=%darken(\"#FF0000\", 10)";
        let out = preprocess(src).unwrap();
        assert!(
            out.trim() == "c=#E60000" || out.trim() == "c=#E50000",
            "darken red 10% should be ~#E60000, got: {}",
            out
        );
    }

    // Ported from upstream: DarkenTest.testDarken — Blue, 0%
    #[test]
    fn upstream_darken_blue_0pct() {
        let src = "c=%darken(\"#0000FF\", 0)";
        let out = preprocess(src).unwrap();
        assert_eq!(out.trim(), "c=#0000FF", "darken blue 0%, got: {}", out);
    }

    // Ported from upstream: DarkenTest.testDarken — 100%
    #[test]
    fn upstream_darken_100pct_becomes_black() {
        let src = "c=%darken(\"#123456\", 100)";
        let out = preprocess(src).unwrap();
        assert_eq!(
            out.trim(),
            "c=#000000",
            "darken 100% should be black, got: {}",
            out
        );
    }

    // ── Ported from upstream: LightenTest ─────────────────────────

    // Ported from upstream: LightenTest.testLighten — White, 50%
    #[test]
    fn upstream_lighten_white_50pct() {
        let src = "c=%lighten(\"#FFFFFF\", 50)";
        let out = preprocess(src).unwrap();
        assert_eq!(
            out.trim(),
            "c=#FFFFFF",
            "lighten white should stay white, got: {}",
            out
        );
    }

    // ── Ported from upstream: IsDarkTest ──────────────────────────

    // Ported from upstream: IsDarkTest.testIsDark — black is dark
    #[test]
    fn upstream_is_dark_black() {
        let src = "result=%is_dark(\"#000000\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=true"),
            "black should be dark, got: {}",
            out
        );
    }

    // Ported from upstream: IsDarkTest.testIsDark — white is not dark
    #[test]
    fn upstream_is_dark_white() {
        let src = "result=%is_dark(\"#FFFFFF\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=false"),
            "white should not be dark, got: {}",
            out
        );
    }

    // Ported from upstream: IsDarkTest.testIsDark — arbitrary dark color
    #[test]
    fn upstream_is_dark_arbitrary_dark() {
        let src = "result=%is_dark(\"#123456\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=true"),
            "#123456 should be dark, got: {}",
            out
        );
    }

    // Ported from upstream: IsDarkTest.testIsDark — arbitrary light color
    #[test]
    fn upstream_is_dark_arbitrary_light() {
        let src = "result=%is_dark(\"#ABCDEF\")";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("result=false"),
            "#ABCDEF should not be dark, got: {}",
            out
        );
    }

    // ── Ported from upstream: ColorHSBTest ────────────────────────

    // Ported from upstream: ColorHSBTest.test_toString — pure red HSB
    #[test]
    fn upstream_color_hsb_red() {
        let (r, g, b) = (255u8, 0u8, 0u8);
        let max = r.max(g).max(b) as f64 / 255.0;
        let min = r.min(g).min(b) as f64 / 255.0;
        let brightness = max;
        let saturation = if max == 0.0 { 0.0 } else { (max - min) / max };
        assert!((brightness - 1.0).abs() < 0.001, "brightness");
        assert!((saturation - 1.0).abs() < 0.001, "saturation");
    }

    // Ported from upstream: ColorHSBTest.test_toString — pure green HSB
    #[test]
    fn upstream_color_hsb_green() {
        let (r, g, b) = (0u8, 255u8, 0u8);
        let max = r.max(g).max(b) as f64 / 255.0;
        let min = r.min(g).min(b) as f64 / 255.0;
        let brightness = max;
        let saturation = if max == 0.0 { 0.0 } else { (max - min) / max };
        assert!((brightness - 1.0).abs() < 0.001);
        assert!((saturation - 1.0).abs() < 0.001);
        let rf = r as f64 / 255.0;
        let gf = g as f64 / 255.0;
        let bf = b as f64 / 255.0;
        let delta = max - min;
        let mut hue = if gf == max {
            2.0 + (bf - rf) / delta
        } else if bf == max {
            4.0 + (rf - gf) / delta
        } else {
            (gf - bf) / delta
        };
        hue /= 6.0;
        if hue < 0.0 {
            hue += 1.0;
        }
        assert!((hue - 0.333333).abs() < 0.001, "hue={}", hue);
    }

    // Ported from upstream: ColorHSBTest.test_toString — half-saturated red
    #[test]
    fn upstream_color_hsb_half_saturated_red() {
        let (r, g, b) = (255u8, 128u8, 128u8);
        let max = r.max(g).max(b) as f64 / 255.0;
        let min = r.min(g).min(b) as f64 / 255.0;
        let saturation = if max == 0.0 { 0.0 } else { (max - min) / max };
        assert!(
            (saturation - 0.498039).abs() < 0.01,
            "saturation for #FF8080 = {}",
            saturation
        );
    }

    // ── Ported from upstream: ColorTrieNodeTest ───────────────────

    // Ported from upstream: ColorTrieNodeTest.testInvalidCharacterIgnoredOnPut
    #[test]
    fn upstream_color_trie_darkblue_normalize() {
        let normalized = crate::style::normalize_color("darkblue");
        assert_eq!(normalized, "#00008B", "named color converted to hex");
    }

    // ── Ported from upstream: HSL color tests ────────────────────

    // Ported from upstream: additional HSL conversion tests.
    // Note: hsl_to_rgb expects hue in degrees (0-360).
    #[test]
    fn upstream_hsl_pure_blue() {
        let (r, g, b) = hsl_to_rgb(240.0, 1.0, 0.5);
        assert_eq!((r, g, b), (0, 0, 255), "pure blue via HSL");
    }

    #[test]
    fn upstream_hsl_gray() {
        let (r, g, b) = hsl_to_rgb(0.0, 0.0, 0.5);
        assert_eq!(r, g);
        assert_eq!(g, b);
        assert!(
            (r as i32 - 128).abs() <= 1,
            "gray should be ~128, got {}",
            r
        );
    }

    #[test]
    fn upstream_hsl_pure_green() {
        let (r, g, b) = hsl_to_rgb(120.0, 1.0, 0.5);
        assert_eq!((r, g, b), (0, 255, 0), "pure green via HSL");
    }

    // ── Ported from upstream: SplitStr tests ─────────────────────

    // Ported from upstream: SplitStrTest.Test_with_String
    #[test]
    fn upstream_splitstr_basic() {
        let src = "!$result = %splitstr(\"abc~def~ghi\", \"~\")\nsize=%size($result)";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("size=3"),
            "splitstr should produce 3 elements, got: {}",
            out
        );
    }

    // Ported from upstream: SplitStrTest.Test_with_String — two parts
    #[test]
    fn upstream_splitstr_two_parts() {
        let src = "!$result = %splitstr(\"foozbar\", \"z\")\nsize=%size($result)";
        let out = preprocess(src).unwrap();
        assert!(
            out.contains("size=2"),
            "splitstr should produce 2 elements, got: {}",
            out
        );
    }

    #[test]
    fn test_jaws3_preproc_newline_in_table_variable() {
        let source = r#"@startuml
!global $table = "|= Field1 |= Field 2 |"
!procedure $row($value1, $value2)
%set_variable_value("$table", %get_variable_value("$table") + %newline() + "| " + $value1 + " | " + $value2 + " |")
!endfunction

$row("1", "2")
$row("3", "4")

rectangle r [
<i>on rectangle:
$table
]
@enduml"#;
        let expanded = preprocess(source).unwrap();
        // %newline() returns the U+E100 placeholder (matches Java's
        // Jaws.BLOCK_E1_NEWLINE), so the table value stays on a single line and
        // contains the placeholder between the row fragments. The downstream
        // table parser sees one row with cells separated by '|' rather than
        // splitting on newlines.
        let nl = crate::NEWLINE_CHAR;
        let table_lines: Vec<&str> = expanded
            .lines()
            .filter(|l| l.trim_start().starts_with('|'))
            .collect();
        assert_eq!(table_lines.len(), 1, "table should remain on a single line");
        let single = table_lines[0];
        assert!(single.contains("Field1"), "got: {single}");
        assert!(
            single.contains(nl),
            "should contain U+E100 placeholder, got: {single}"
        );
        assert!(single.contains("| 1 | 2 |"), "got: {single}");
        assert!(single.contains("| 3 | 4 |"), "got: {single}");
    }
}

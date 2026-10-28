//! App commands scripts declare: `ui.command { id, title, menu, submenu, key,
//! checked, run }`.
//!
//! Each run declares its commands afresh; after a successful load the host
//! publishes them ([`crate::ScriptHost::commands`]) for the application to
//! show as menu items with key equivalents, and calls them back with
//! [`crate::ScriptHost::run_command`]. Menus reach the app wherever focus is,
//! a terminal or a web page included.
//!
//! A plugin's commands are its own: their ids are `<plugin>/<id>`, and a
//! shortcut the app's commands (or an earlier plugin's) have stays theirs,
//! so a plugin neither replaces nor breaks the app's ⌘C.
use crate::{Vm, node::env};
use mlua::{Function, Lua, Table};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};
use zgx::reactive::{Effect, Runtime, Signal};

/// A command as the application shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    /// Unique; empty for a separator.
    pub id: String,
    /// What the menu item says.
    pub title: String,
    /// The menu it goes in ("File", "Edit", "View", "Window", or any other).
    pub menu: String,
    /// Its shortcut: modifiers and a key joined by `+`, as `cmd+shift+d`,
    /// `cmd+alt+arrowleft`, `cmd+1`, `ctrl+tab`.
    pub key: Option<String>,
    /// A separator line, not a command.
    pub separator: bool,
    /// The menu it nests in inside `menu` ("Broadcast Input" in "Shell").
    pub submenu: Option<String>,
    /// A check mark beside it: `Some(true)` checked, `Some(false)` checkable
    /// and unchecked, `None` not a toggle. Kept current as what the
    /// script's `checked` reads changes.
    pub checked: Option<bool>,
}

type Runner = (Function, Weak<Vm>);
/// A declared command, its runner, the plugin that declared it, and its
/// check mark as the script keeps it (`checked` given as a function).
type Pending = (
    Command,
    Option<Runner>,
    Option<Rc<str>>,
    Option<Signal<crate::Value>>,
);
pub(crate) struct Commands {
    runtime: Runtime,
    pending: RefCell<Vec<Pending>>,
    runners: RefCell<HashMap<String, Runner>>,
    /// What keeps the published commands' check marks current.
    checks: RefCell<Vec<Effect>>,
    pub list: Signal<Vec<Command>>,
}
impl Commands {
    pub fn new(runtime: &Runtime) -> Self {
        Self {
            runtime: runtime.clone(),
            pending: RefCell::default(),
            runners: RefCell::default(),
            checks: RefCell::default(),
            list: runtime.signal(Vec::new()),
        }
    }
    /// A load begins: what it declares replaces what is published.
    pub fn begin(&self) {
        self.pending.borrow_mut().clear();
    }
    /// The load succeeded: publish what it declared.
    pub fn publish(&self) {
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        // The app's shortcuts first, then each plugin's not yet taken.
        let mut taken: std::collections::HashSet<String> = pending
            .iter()
            .filter(|(_, _, plugin, _)| plugin.is_none())
            .filter_map(|(c, _, _, _)| c.key.as_deref().map(shortcut))
            .collect();
        let mut runners = HashMap::new();
        let mut list = Vec::with_capacity(pending.len());
        let mut checks = Vec::new();
        for (mut command, runner, plugin, checked) in pending {
            if plugin.is_some()
                && let Some(key) = &command.key
                && !taken.insert(shortcut(key))
            {
                command.key = None;
            }
            if let Some(runner) = runner {
                runners.insert(command.id.clone(), runner);
            }
            if let Some(checked) = checked {
                command.checked = Some(checked.with_untracked(crate::Value::truthy));
                checks.push((command.id.clone(), checked));
            }
            list.push(command);
        }
        *self.runners.borrow_mut() = runners;
        if self.list.with_untracked(|old| *old != list) {
            self.list.set(list);
        }
        // Each check mark follows its value: the published command changes
        // in place, and what shows the menus shows it.
        let effects = checks
            .into_iter()
            .map(|(id, checked)| {
                let list = self.list.clone();
                self.runtime.effect(move || {
                    let now = checked.with(crate::Value::truthy);
                    let stale = list.with_untracked(|commands| {
                        commands
                            .iter()
                            .any(|c| c.id == id && c.checked != Some(now))
                    });
                    if stale {
                        list.update(|commands| {
                            for command in commands.iter_mut().filter(|c| c.id == id) {
                                command.checked = Some(now);
                            }
                        });
                    }
                })
            })
            .collect();
        *self.checks.borrow_mut() = effects;
    }
    /// Run the command `id`; whether one ran.
    pub fn run(&self, id: &str) -> bool {
        let runner = self.runners.borrow().get(id).cloned();
        let Some((f, vm)) = runner else {
            return false;
        };
        let Some(vm) = vm.upgrade() else {
            return false;
        };
        if let Err(error) = vm.runtime.clone().batch(|| vm.call::<()>(&f, ())) {
            vm.report(error);
        }
        true
    }
}

/// A shortcut as compared: `Shift+Cmd+D` is `cmd+shift+d`.
fn shortcut(key: &str) -> String {
    let mut parts: Vec<String> = key.split('+').map(|p| p.trim().to_lowercase()).collect();
    let key = parts.pop().unwrap_or_default();
    parts.sort();
    parts.dedup();
    parts.push(key);
    parts.join("+")
}

/// Installs `ui.command`, and `ui.shortcut_text`: keys (`cmd+z`) as this
/// platform shows them (⌘Z, or Ctrl+Z), for a tooltip or a list of keys.
pub(crate) fn install(lua: &Lua, ui: &Table, commands: Rc<Commands>) -> mlua::Result<()> {
    ui.set(
        "shortcut_text",
        lua.create_function(|_, keys: String| Ok(crate::lower::shortcut_label(&keys)))?,
    )?;
    ui.set(
        "command",
        lua.create_function(move |lua, table: Table| {
            let menu = table
                .get::<Option<String>>("menu")?
                .unwrap_or_else(|| "File".into());
            let submenu = table.get::<Option<String>>("submenu")?;
            if table.get::<Option<bool>>("separator")?.unwrap_or(false) {
                commands.pending.borrow_mut().push((
                    Command {
                        id: String::new(),
                        title: String::new(),
                        menu,
                        key: None,
                        separator: true,
                        submenu,
                        checked: None,
                    },
                    None,
                    None,
                    None,
                ));
                return Ok(());
            }
            let env = env(lua)?;
            let plugin = match &*env.grants {
                crate::Grants::All => None,
                crate::Grants::Only(_) => Some(env.owner.clone()),
            };
            let id: String = table
                .get::<Option<String>>("id")?
                .ok_or_else(|| mlua::Error::runtime("ui.command needs an `id`"))?;
            let id = match &plugin {
                Some(plugin) => format!("{plugin}/{id}"),
                None => id,
            };
            let title = table
                .get::<Option<String>>("title")?
                .unwrap_or_else(|| id.clone());
            let run: Function = table
                .get::<Option<Function>>("run")?
                .ok_or_else(|| mlua::Error::runtime(format!("ui.command `{id}` needs `run`")))?;
            let key = table.get::<Option<String>>("key")?;
            // A check mark: fixed, or kept as a function's value is.
            let (checked, live) = match table.get::<mlua::Value>("checked")? {
                mlua::Value::Nil => (None, None),
                mlua::Value::Boolean(on) => (Some(on), None),
                mlua::Value::Function(f) => (None, Some(crate::node::computed(lua, &env, f))),
                _ => {
                    return Err(mlua::Error::runtime(format!(
                        "ui.command `{id}`: `checked` is a boolean or a function"
                    )));
                }
            };
            let vm = env.vm.clone();
            let mut pending = commands.pending.borrow_mut();
            if pending.iter().any(|(c, _, _, _)| c.id == id) {
                return Err(mlua::Error::runtime(format!(
                    "ui.command `{id}` is declared twice"
                )));
            }
            pending.push((
                Command {
                    id,
                    title,
                    menu,
                    key,
                    separator: false,
                    submenu,
                    checked,
                },
                Some((run, vm)),
                plugin,
                live,
            ));
            Ok(())
        })?,
    )
}

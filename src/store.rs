use serde::{Deserialize, Serialize};
use serde_json;
use slotmap::{DefaultKey, SlotMap};
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Serialize, Deserialize)]
pub struct Todo {
    pub id: String,
    pub description: String,
    pub due: Option<SystemTime>,
    pub effort: usize,
    pub urgency: usize,
    pub pending: bool,
    pub children: Vec<Todo>,
}

#[derive(Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub description: String,
    pub children: Vec<Workspace>,
    pub todos: Vec<Todo>,
}

impl Workspace {
    pub fn get_mut_todo(&mut self, selected: &[String]) -> Option<&mut Todo> {
        let mut selected_iter = selected.iter();
        let first_item = selected_iter.next()?;

        let mut todo = self.todos.iter_mut().find(|t| t.id == *first_item)?;

        while let Some(id) = selected_iter.next() {
            todo = todo.children.iter_mut().find(|t| t.id == *id)?;
        }

        Some(todo)
    }

    pub fn get_todo(&self, selected: &[String]) -> Option<&Todo> {
        let mut selected_iter = selected.iter();
        let first_item = selected_iter.next()?;

        let mut todo = self.todos.iter().find(|t| t.id == *first_item)?;

        while let Some(id) = selected_iter.next() {
            todo = todo.children.iter().find(|t| t.id == *id)?;
        }

        Some(todo)
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct Store {
    pub workspaces: Vec<Workspace>,
}

impl Store {
    pub fn from_json_file(path: &PathBuf) -> io::Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let store = serde_json::from_reader(reader)?;
        Ok(store)
    }

    pub fn to_json_file(&self, path: &PathBuf) -> io::Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &self)?;
        Ok(())
    }

    pub fn get_mut_workflow(&mut self, selection: Vec<String>) -> Option<&mut Workspace> {
        let mut selection_iter = selection.iter();
        let first_item = selection_iter.next()?;

        let mut workspace = self.workspaces.iter_mut().find(|w| w.id == *first_item)?;

        while let Some(id) = selection_iter.next() {
            workspace = workspace.children.iter_mut().find(|w| w.id == *id)?;
        }

        Some(workspace)
    }

    pub fn get_workflow(&self, selection: &[String]) -> Option<&Workspace> {
        let mut selection_iter = selection.iter();
        let first_item = selection_iter.next()?;

        let mut workspace = self.workspaces.iter().find(|w| w.id == *first_item)?;

        while let Some(id) = selection_iter.next() {
            workspace = workspace.children.iter().find(|w| w.id == *id)?;
        }

        Some(workspace)
    }
}

#[derive(Clone)]
pub struct WorkspaceItem {
    pub id: String,
    pub description: String,
    pub todos: Vec<DefaultKey>,
    pub children: Vec<DefaultKey>,
}

#[derive(Clone)]
pub struct TodoItem {
    pub id: String,
    pub description: String,
    pub due: Option<SystemTime>,
    pub effort: usize,
    pub urgency: usize,
    pub pending: bool,
    pub children: Vec<DefaultKey>,
}

pub struct SlotMapStore {
    pub workspaces_map: SlotMap<DefaultKey, WorkspaceItem>,
    pub todos_map: SlotMap<DefaultKey, TodoItem>,
    pub root_workspaces: Vec<DefaultKey>,
}

impl SlotMapStore {
    fn add_todo(todos_map: &mut SlotMap<DefaultKey, TodoItem>, t: &Todo) -> DefaultKey {
        let mut todo_item = TodoItem {
            id: t.id.clone(),
            effort: t.effort,
            urgency: t.urgency,
            pending: t.pending,
            children: Vec::new(),
            description: t.description.clone(),
            due: t.due,
        };

        todo_item.children = t
            .children
            .iter()
            .map(|t| Self::add_todo(todos_map, t))
            .collect();

        return todos_map.insert(todo_item);
    }

    fn add_workspace(
        workspace_map: &mut SlotMap<DefaultKey, WorkspaceItem>,
        todos_map: &mut SlotMap<DefaultKey, TodoItem>,
        w: &Workspace,
    ) -> DefaultKey {
        let mut ws_item = WorkspaceItem {
            id: w.id.clone(),
            description: w.description.clone(),
            todos: Vec::new(),
            children: Vec::new(),
        };

        ws_item.children = w
            .children
            .iter()
            .map(|c| Self::add_workspace(workspace_map, todos_map, c))
            .collect();

        ws_item.todos = w
            .todos
            .iter()
            .map(|t| Self::add_todo(todos_map, t))
            .collect();

        return workspace_map.insert(ws_item);
    }

    pub fn from_store(store: &Store) -> Self {
        let mut workspaces_map = SlotMap::new();
        let mut todos_map = SlotMap::new();
        let root_workspaces: Vec<DefaultKey> = store
            .workspaces
            .iter()
            .map(|w| Self::add_workspace(&mut workspaces_map, &mut todos_map, w))
            .collect();

        Self {
            root_workspaces,
            workspaces_map,
            todos_map,
        }
    }

    fn create_todo(&self, key: DefaultKey) -> Todo {
        let t = self.todos_map.get(key).unwrap();
        Todo {
            id: t.id.clone(),
            description: t.description.clone(),
            children: t.children.iter().map(|k| self.create_todo(*k)).collect(),
            due: t.due,
            effort: t.effort,
            urgency: t.urgency,
            pending: t.pending,
        }
    }

    fn create_workspace(&self, key: DefaultKey) -> Workspace {
        let ws = self.workspaces_map.get(key).unwrap();
        Workspace {
            id: ws.id.clone(),
            description: ws.description.clone(),
            children: ws
                .children
                .iter()
                .map(|k| self.create_workspace(*k))
                .collect(),
            todos: ws.todos.iter().map(|k| self.create_todo(*k)).collect(),
        }
    }

    pub fn get_store(&self) -> Store {
        Store {
            workspaces: self
                .root_workspaces
                .iter()
                .map(|k| self.create_workspace(*k))
                .collect(),
        }
    }
}

use ratatui::text::Span;
use ratatui::widgets::{ListState, Padding, Row, Table, TableState};
use slotmap::{DefaultKey, SlotMap};
use std::collections::HashSet;
use std::sync::mpsc;
use uuid::Uuid;

use crate::colors::Theme;
use crate::store::{self, SlotMapStore};
use crate::store::{Store, TodoItem, WorkspaceItem};
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, List, ListItem},
    DefaultTerminal, Frame,
};
use tui_input::{backend::crossterm::EventHandler, Input};

#[derive(PartialEq, Eq)]
enum Screen {
    Workspaces,
    Todos,
}

const PRIORITIES: [&'static str; 4] = ["󰯬", "󰯯", "󰯲", "󰯵"];
const PRIORITY_COLORS: [Color; 4] = [Color::Green, Color::Yellow, Color::Magenta, Color::Red];

pub struct App {
    theme: crate::colors::Theme,
    running: bool,
    slot_map_store: store::SlotMapStore,
    slot_tree_state: SlotTreeState,
    input: Input,
    new_editing_id: Option<DefaultKey>,
    active_screen: Screen,
    sorting: SortingItem,
    tx: mpsc::Sender<crossterm::event::Event>,
    rx: mpsc::Receiver<crossterm::event::Event>,
    clipboard_todos: Vec<DefaultKey>,
    clipboard_workspaces: Vec<DefaultKey>,
    search_mode: bool,
    search_str: String,
    search_matches: Vec<DefaultKey>,
    current_match_index: usize,
}

enum SortingItem {
    Workspace(DefaultKey),
    Todo(DefaultKey),
    None,
}

impl App {
    /// Construct a new instance of [`App`].
    pub fn new(store: Store, theme: Theme) -> Self {
        let (tx, rx) = mpsc::channel();
        let slot_map_store = store::SlotMapStore::from_store(&store);
        Self {
            theme,
            running: false,
            new_editing_id: None,
            slot_tree_state: SlotTreeState::default(),
            slot_map_store,
            input: Input::default(),
            sorting: SortingItem::None,
            active_screen: Screen::Workspaces,
            clipboard_todos: Vec::new(),
            clipboard_workspaces: Vec::new(),
            tx,
            rx,
            search_mode: false,
            search_str: String::new(),
            search_matches: Vec::new(),
            current_match_index: 0,
        }
    }

    /// Run the application's main loop.
    pub fn run(&mut self, mut terminal: DefaultTerminal) -> Result<()> {
        self.running = true;

        let tx = self.tx.clone();
        std::thread::spawn(move || {
            get_crossterm_events(tx.clone()).unwrap();
        });

        while self.running {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    pub fn get_store(&self) -> Store {
        self.slot_map_store.get_store()
    }

    pub fn sort_todos(&mut self, todos: &mut Vec<DefaultKey>, n: char) {
        todos.sort_by(|a, b| {
            let a = self.slot_map_store.todos_map.get(*a).unwrap();
            let b = self.slot_map_store.todos_map.get(*b).unwrap();

            match n {
                '2' => a.description.cmp(&b.description),
                '3' => a.pending.cmp(&b.pending),
                '4' => a.urgency.cmp(&b.urgency),
                _ => a.description.cmp(&b.description),
            }
        });
    }

    /// Renders the user interface.
    ///
    /// This is where you add new widgets. See the following resources for more information:
    /// - <https://docs.rs/ratatui/latest/ratatui/widgets/index.html>
    /// - <https://github.com/ratatui/ratatui/tree/master/examples>
    fn draw(&mut self, frame: &mut Frame) {
        let main_vertical_areas: [Rect; 2] =
            Layout::vertical(vec![Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
        let main_areas: [Rect; 2] =
            Layout::horizontal(vec![Constraint::Percentage(20), Constraint::Fill(20)])
                .areas(main_vertical_areas[0]);

        self.slot_tree_state
            .update_workspace_tree_state(&self.slot_map_store);

        self.render_workspaces(frame, main_areas[0]);
        self.render_todos(frame, main_areas[1]);
        self.render_footer(frame, main_vertical_areas[1]);
    }

    fn render_footer(&mut self, frame: &mut Frame, area: Rect) {
        let mut line = Line::default();
        if self.search_mode {
            line.push_span(Span::raw(" Search: ").bg(Color::Blue).fg(self.theme.text));
            line.push_span(Span::raw(format!(" {}", &self.search_str)));
        } else {
            match self.sorting {
                SortingItem::None => {
                    line.push_span(Span::raw(" INSERT ").bg(Color::Green).fg(Color::Black));
                }
                SortingItem::Todo(_) => {
                    line.push_span(Span::raw(" Sort by: ").bg(Color::Blue).fg(self.theme.text));
                    line.push_span(Span::raw(" 1:Reverse "));
                    line.push_span(Span::raw(" 2:Description "));
                    line.push_span(Span::raw(" 3:Pending "));
                    line.push_span(Span::raw(" 4:Urgency "));
                }
                SortingItem::Workspace(_) => {
                    line.push_span(Span::raw(" Sort by: ").bg(Color::Cyan).fg(Color::Black));
                    line.push_span(Span::raw(" 1:Reverse "));
                    line.push_span(Span::raw(" 2:Description "));
                }
            }
        }

        frame.render_widget(line, area);
    }

    fn render_workspaces(&mut self, frame: &mut Frame, area: Rect) {
        let mut items: Vec<ListItem> = Vec::new();

        self.slot_tree_state.ws_tree.iter().for_each(|w| {
            let workspace = self.slot_map_store.workspaces_map.get(w.key).unwrap();
            let mut item = ListItem::new(format!(
                "{}{}{}",
                "  ".repeat(w.depth),
                workspace.description.clone(),
                if workspace.children.is_empty() || self.slot_tree_state.ws_opened.contains(&w.key)
                {
                    "".to_string()
                } else {
                    format!("({})", workspace.children.len())
                }
            ));

            let mut item_style = Style::default();
            if let Some(selected) = self.slot_tree_state.selected_workspace {
                if selected == w.key {
                    item_style = item_style.fg(self.theme.text).bg(self.theme.item_highlight);
                }
            }

            // Highlight multi-selected items
            if self
                .slot_tree_state
                .multi_selected_workspaces
                .contains(&w.key)
            {
                item_style = item_style.fg(Color::Yellow);
            }

            item = item.style(item_style);
            items.push(item);
        });

        let block = self.get_title_block(" Workspaces ", self.active_screen == Screen::Workspaces);

        // Render the input
        if let Some(editing_id) = self.new_editing_id {
            let cursor_x = self.input.visual_cursor();

            let ind = self
                .slot_tree_state
                .ws_tree
                .iter()
                .position(|w| w.key == editing_id);

            if let Some(ind) = ind {
                let mut list_item = ListItem::new(format!(
                    "{}{}",
                    "  ".repeat(self.slot_tree_state.ws_tree[ind].depth),
                    self.input.value()
                ));

                // TODO: Refactor these out
                if let Some(selected) = self.slot_tree_state.selected_workspace {
                    if selected == editing_id {
                        list_item = list_item.style(
                            Style::default()
                                .fg(self.theme.text)
                                .bg(self.theme.item_highlight),
                        );
                    }
                }

                items[ind] = list_item;

                let y = ind;
                let x = self.slot_tree_state.ws_tree[ind].depth * 2;
                let inner_area = block.inner(area);
                frame.set_cursor_position(Position::new(
                    inner_area.x + (x + cursor_x) as u16,
                    inner_area.y + y as u16,
                ));
            }
        }

        let widget = List::new(items).block(block);

        let mut list_state = ListState::default();
        if let Some(selected_workspace) = self.slot_tree_state.selected_workspace {
            let index = self
                .slot_tree_state
                .ws_tree
                .iter()
                .position(|w| w.key == selected_workspace)
                .unwrap();
            list_state.select(Some(index));
        }

        frame.render_stateful_widget(widget, area, &mut list_state);
    }

    fn get_title_block(&self, title: &str, active: bool) -> Block {
        let styles = if active {
            (
                Style::default()
                    .fg(self.theme.text_dark)
                    .bg(self.theme.active_highlight),
                Style::default().fg(self.theme.active_highlight),
            )
        } else {
            (
                Style::default()
                    .fg(self.theme.highlight_text_secondary)
                    .bg(self.theme.inactive_highlight),
                Style::default().fg(self.theme.inactive_highlight),
            )
        };

        let block = Block::bordered()
            .title(title.to_string())
            .title_style(styles.0)
            .border_style(styles.1)
            .padding(Padding::uniform(1));

        return block;
    }

    fn render_todos(&mut self, frame: &mut Frame, area: Rect) {
        let mut rows: Vec<Row> = Vec::new();
        self.slot_tree_state.todo_tree.iter().for_each(|t| {
            let todo = self.slot_map_store.todos_map.get(t.key).unwrap();

            let icon = if todo.pending { " " } else { " " };
            let mut todo_desc: Span = todo.description.clone().into();
            let mut pre_desc = Span::from(format!("{}{} ", "  ".repeat(t.depth), icon))
                .style(Style::new().fg(Color::Yellow));

            if !todo.pending {
                todo_desc =
                    todo_desc.style(Style::new().fg(self.theme.text_completed).crossed_out());
                pre_desc = pre_desc.style(Style::new().fg(Color::Green));
            }

            if self.search_matches.contains(&t.key) {
                todo_desc = todo_desc.style(Style::new().fg(Color::Yellow).bold());
            }

            let mut todo_line = Line::from(pre_desc);
            todo_line.push_span(todo_desc);

            // show children count
            if !todo.children.is_empty() {
                let mut done_count = 0;
                todo.children.iter().for_each(|child_key| {
                    let todo = self.slot_map_store.todos_map.get(*child_key).unwrap();
                    if !todo.pending {
                        done_count += 1;
                    }
                });

                todo_line.push_span(Span::styled(
                    format!(" {}{}/{}", icon, done_count, todo.children.len()),
                    Style::default().fg(Color::LightGreen),
                ));
            }

            let mut priority = Line::from(PRIORITIES[todo.urgency as usize]);
            priority = priority.style(Style::new().fg(PRIORITY_COLORS[todo.urgency as usize]));

            let mut row_style = Style::default();
            let mut row = Row::new(vec![todo_line, priority.into()]);
            if let Some(selected) = self.slot_tree_state.selected_todo {
                if selected == t.key {
                    row_style = row_style.bg(self.theme.item_highlight);
                }
            }

            // Highlight multi-selected items
            if self.slot_tree_state.multi_selected_todos.contains(&t.key) {
                row_style = row_style.fg(Color::Yellow);
            }

            row = row.style(row_style);

            rows.push(row);
        });

        let todos_title = " Todos ".to_string();

        let block = self.get_title_block(todos_title.as_str(), self.active_screen == Screen::Todos);

        // Render the input
        if let Some(editing_id) = self.new_editing_id {
            let cursor_x = self.input.visual_cursor();

            let ind = self
                .slot_tree_state
                .todo_tree
                .iter()
                .position(|w| w.key == editing_id);

            if let Some(ind) = ind {
                let todo_desc = format!(
                    "{}{} {}",
                    "  ".repeat(self.slot_tree_state.todo_tree[ind].depth),
                    " ",
                    self.input.value()
                );
                let mut row = Row::new(vec![todo_desc]);
                row = row.style(
                    Style::default()
                        .fg(self.theme.text)
                        .bg(self.theme.item_highlight),
                );
                rows[ind] = row;

                let y = ind;
                let x = self.slot_tree_state.todo_tree[ind].depth * 2;
                let inner_area = block.inner(area);
                frame.set_cursor_position(Position::new(
                    inner_area.x + (x + cursor_x) as u16 + 3,
                    inner_area.y + y as u16,
                ));
            }
        }

        let widths = [Constraint::Fill(5), Constraint::Length(2)];

        let widget = Table::new(rows, widths).block(block);

        let mut table_state = TableState::default();
        if let Some(selected_todo) = self.slot_tree_state.selected_todo {
            let index = self
                .slot_tree_state
                .todo_tree
                .iter()
                .position(|w| w.key == selected_todo)
                .unwrap();
            table_state.select(Some(index));
        }

        frame.render_stateful_widget(widget, area, &mut table_state);
    }

    fn handle_events(&mut self) -> Result<()> {
        let event = self.rx.recv()?;
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                self.handle_crossterm_events(event)?
            }
        }
        Ok(())
    }

    fn update_search_matches(&mut self) {
        self.search_matches.clear();
        self.current_match_index = 0;

        if self.search_str.is_empty() {
            return;
        }

        if let Some(workspace_key) = self.slot_tree_state.selected_workspace {
            let workspace = self
                .slot_map_store
                .workspaces_map
                .get(workspace_key)
                .unwrap();

            // Helper function to recursively search todos
            fn search_todos(
                todos_map: &SlotMap<DefaultKey, TodoItem>,
                todo_key: DefaultKey,
                search_str: &str,
                matches: &mut Vec<DefaultKey>,
                todos_containing_matches: &mut Vec<DefaultKey>,
            ) -> bool {
                let todo = todos_map.get(todo_key).unwrap();

                let mut contains_match = false;

                if todo
                    .description
                    .to_lowercase()
                    .contains(&search_str.to_lowercase())
                {
                    matches.push(todo_key);
                    contains_match = true;
                }

                // Search in children
                for child_key in &todo.children {
                    if search_todos(
                        todos_map,
                        *child_key,
                        search_str,
                        matches,
                        todos_containing_matches,
                    ) {
                        contains_match = true;
                    }
                }

                if contains_match {
                    todos_containing_matches.push(todo_key);
                }

                return contains_match;
            }

            // Search in workspace's direct todos
            let mut todos_containing_matches: Vec<DefaultKey> = Vec::new();
            for todo_key in &workspace.todos {
                search_todos(
                    &self.slot_map_store.todos_map,
                    *todo_key,
                    &self.search_str,
                    &mut self.search_matches,
                    &mut todos_containing_matches,
                );
            }

            self.slot_tree_state.todo_opened.clear();
            for todo_key in &todos_containing_matches {
                self.slot_tree_state.todo_opened.insert(*todo_key);
            }
        }
    }

    fn clone_todo(&mut self, todo_key: DefaultKey) -> DefaultKey {
        let old_todo = self.slot_map_store.todos_map.get(todo_key).unwrap().clone();

        let mut todo = TodoItem {
            id: Uuid::new_v4().to_string(),
            description: old_todo.description.clone(),
            pending: old_todo.pending,
            urgency: old_todo.urgency,
            effort: old_todo.effort,
            due: old_todo.due,
            children: Vec::new(),
        };

        for todo_key in old_todo.children.iter() {
            let key = self.clone_todo(*todo_key);
            todo.children.push(key);
        }

        return self.slot_map_store.todos_map.insert(todo);
    }

    fn clone_workspace(&mut self, workspace_key: DefaultKey) -> DefaultKey {
        let old_workspace = self
            .slot_map_store
            .workspaces_map
            .get(workspace_key)
            .unwrap()
            .clone();

        let mut workspace = WorkspaceItem {
            id: Uuid::new_v4().to_string(),
            description: old_workspace.description.clone(),
            children: Vec::new(),
            todos: Vec::new(),
        };

        for child_key in old_workspace.children.iter() {
            let key = self.clone_workspace(*child_key);
            workspace.children.push(key);
        }

        for todo_key in old_workspace.todos.iter() {
            let key = self.clone_todo(*todo_key);
            workspace.todos.push(key);
        }

        return self.slot_map_store.workspaces_map.insert(workspace);
    }

    fn paste_todo_as_child(&mut self, key: DefaultKey, selected: DefaultKey) {
        let new_todos_key = self.clone_todo(key);
        let todo = self.slot_map_store.todos_map.get_mut(selected).unwrap();
        todo.children.push(new_todos_key);
    }

    fn delete_todo(&mut self, selected: DefaultKey) {
        let todo_tree_item = self
            .slot_tree_state
            .todo_tree
            .iter()
            .find(|w| w.key == selected)
            .unwrap();

        if let Some(parent) = todo_tree_item.parent {
            let parent = self.slot_map_store.todos_map.get_mut(parent).unwrap();

            parent
                .children
                .remove(parent.children.iter().position(|w| w == &selected).unwrap());
        } else {
            let workspace = self
                .slot_map_store
                .workspaces_map
                .get_mut(self.slot_tree_state.selected_workspace.unwrap())
                .unwrap();
            workspace
                .todos
                .remove(workspace.todos.iter().position(|w| w == &selected).unwrap());
        }

        let index = self
            .slot_tree_state
            .todo_tree
            .iter()
            .position(|t| t.key == selected)
            .unwrap();

        self.slot_tree_state
            .update_workspace_tree_state(&self.slot_map_store);

        if self.slot_tree_state.todo_tree.is_empty() {
            self.slot_tree_state.selected_todo = None;
        } else {
            self.slot_tree_state.selected_todo = Some(
                self.slot_tree_state
                    .todo_tree
                    .get(index.min(self.slot_tree_state.todo_tree.len() - 1))
                    .unwrap()
                    .key,
            );
        }
    }

    fn delete_workspace(&mut self, selected: DefaultKey) {
        let ws_tree_item = self
            .slot_tree_state
            .ws_tree
            .iter()
            .find(|w| w.key == selected)
            .unwrap();

        if let Some(parent) = ws_tree_item.parent {
            let parent = self.slot_map_store.workspaces_map.get_mut(parent).unwrap();
            parent
                .children
                .remove(parent.children.iter().position(|w| w == &selected).unwrap());
        } else {
            self.slot_map_store.root_workspaces.remove(
                self.slot_map_store
                    .root_workspaces
                    .iter()
                    .position(|w| w == &selected)
                    .unwrap(),
            );
        }

        let index = self
            .slot_tree_state
            .ws_tree
            .iter()
            .position(|t| t.key == selected)
            .unwrap();

        self.slot_tree_state
            .update_workspace_tree_state(&self.slot_map_store);

        if self.slot_tree_state.ws_tree.is_empty() {
            self.slot_tree_state.selected_workspace = None;
        } else {
            self.slot_tree_state.selected_workspace = Some(
                self.slot_tree_state
                    .ws_tree
                    .get(index.min(self.slot_tree_state.ws_tree.len() - 1))
                    .unwrap()
                    .key,
            );
        }

        // Clear multi-selection when workspace changes due to deletion
        self.clear_multi_selection_when_workspace_changes();
    }

    fn paste_workspace_as_child(&mut self, key: DefaultKey, selected: DefaultKey) {
        let new_workspace_key = self.clone_workspace(key);
        let workspace = self
            .slot_map_store
            .workspaces_map
            .get_mut(selected)
            .unwrap();
        workspace.children.push(new_workspace_key);
    }

    fn handle_workspace_key_event(&mut self, key: KeyEvent) {
        let new_editing_id = self.new_editing_id.clone();

        if let SortingItem::Workspace(workspace_key) = self.sorting {
            match (key.modifiers, key.code) {
                (_, KeyCode::Char('1')) => {
                    let parent_key = self
                        .slot_tree_state
                        .ws_tree
                        .iter()
                        .find(|w| w.key == workspace_key)
                        .unwrap()
                        .parent;

                    if let Some(parent_key) = parent_key {
                        let mut children = self
                            .slot_map_store
                            .workspaces_map
                            .get(parent_key)
                            .unwrap()
                            .children
                            .clone();

                        children.reverse();

                        self.slot_map_store
                            .workspaces_map
                            .get_mut(parent_key)
                            .unwrap()
                            .children = children;
                    } else {
                        self.slot_map_store.root_workspaces.reverse()
                    }

                    self.sorting = SortingItem::None;
                }
                (_, KeyCode::Char('2')) => {
                    let parent_key = self
                        .slot_tree_state
                        .ws_tree
                        .iter()
                        .find(|w| w.key == workspace_key)
                        .unwrap()
                        .parent;

                    if let Some(parent_key) = parent_key {
                        let mut children = self
                            .slot_map_store
                            .workspaces_map
                            .get(parent_key)
                            .unwrap()
                            .children
                            .clone();

                        children.sort_by(|a, b| {
                            let a = self.slot_map_store.workspaces_map.get(*a).unwrap();
                            let b = self.slot_map_store.workspaces_map.get(*b).unwrap();

                            b.description.cmp(&a.description)
                        });
                        self.slot_map_store
                            .workspaces_map
                            .get_mut(parent_key)
                            .unwrap()
                            .children = children;
                    } else {
                        self.slot_map_store.root_workspaces.sort_by(|a, b| {
                            let a = self.slot_map_store.workspaces_map.get(*a).unwrap();
                            let b = self.slot_map_store.workspaces_map.get(*b).unwrap();

                            b.description.cmp(&a.description)
                        });
                    }

                    self.sorting = SortingItem::None;
                }
                _ => {}
            }

            return;
        }

        match new_editing_id {
            Some(id) => {
                match (key.modifiers, key.code) {
                    (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => self.quit(),

                    (_, KeyCode::Esc) | (_, KeyCode::Enter) => {
                        let workspace = self.slot_map_store.workspaces_map.get_mut(id).unwrap();
                        workspace.description = self.input.value().to_string();
                        self.new_editing_id = None;
                    }

                    _ => {
                        self.input.handle_event(&crossterm::event::Event::Key(key));
                    }
                };
            }
            None => match (key.modifiers, key.code) {
                (_, KeyCode::Esc | KeyCode::Char('q'))
                | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => self.quit(),

                (_, KeyCode::Tab) => self.active_screen = Screen::Todos,

                (_, KeyCode::Char('j')) => {
                    let old_workspace = self.slot_tree_state.selected_workspace;
                    if let Some(selected) = self.slot_tree_state.selected_workspace {
                        let index = self
                            .slot_tree_state
                            .ws_tree
                            .iter()
                            .position(|w| w.key == selected)
                            .unwrap();
                        if (index + 1) < self.slot_tree_state.ws_tree.len() {
                            self.slot_tree_state.selected_workspace =
                                Some(self.slot_tree_state.ws_tree[index + 1].key);
                        }
                    } else {
                        self.slot_tree_state.selected_workspace =
                            self.slot_tree_state.ws_tree.first().map(|w| w.key);
                    }
                    self.slot_tree_state.selected_todo = None;

                    // Clear multi-selection when workspace changes
                    if old_workspace != self.slot_tree_state.selected_workspace {
                        self.clear_multi_selection_when_workspace_changes();
                    }
                }

                (_, KeyCode::Char('k')) => {
                    let old_workspace = self.slot_tree_state.selected_workspace;
                    if let Some(selected) = self.slot_tree_state.selected_workspace {
                        let index = self
                            .slot_tree_state
                            .ws_tree
                            .iter()
                            .position(|w| w.key == selected)
                            .unwrap();
                        if index > 0 {
                            self.slot_tree_state.selected_workspace =
                                Some(self.slot_tree_state.ws_tree[index - 1].key);
                        }
                    }
                    self.slot_tree_state.selected_todo = None;

                    // Clear multi-selection when workspace changes
                    if old_workspace != self.slot_tree_state.selected_workspace {
                        self.clear_multi_selection_when_workspace_changes();
                    }
                }

                (_, KeyCode::Char('K')) => {
                    if let Some(selected) = self.slot_tree_state.selected_workspace {
                        let parent = self
                            .slot_tree_state
                            .ws_tree
                            .iter()
                            .find(|w| w.key == selected)
                            .unwrap()
                            .parent;

                        if let Some(parent_key) = parent {
                            let parent = self
                                .slot_map_store
                                .workspaces_map
                                .get_mut(parent_key)
                                .unwrap();
                            let ind = parent.children.iter().position(|k| *k == selected).unwrap();

                            if ind > 0 {
                                parent.children.swap(ind, ind - 1);
                            }
                        } else {
                            let ind = self
                                .slot_map_store
                                .root_workspaces
                                .iter()
                                .position(|k| *k == selected)
                                .unwrap();
                            if ind > 0 {
                                self.slot_map_store.root_workspaces.swap(ind, ind - 1);
                            }
                        }
                    }
                }

                (_, KeyCode::Char('J')) => {
                    if let Some(selected) = self.slot_tree_state.selected_workspace {
                        let parent = self
                            .slot_tree_state
                            .ws_tree
                            .iter()
                            .find(|w| w.key == selected)
                            .unwrap()
                            .parent;

                        if let Some(parent_key) = parent {
                            let parent = self
                                .slot_map_store
                                .workspaces_map
                                .get_mut(parent_key)
                                .unwrap();
                            let ind = parent.children.iter().position(|k| *k == selected).unwrap();

                            if ind < parent.children.len() - 1 {
                                parent.children.swap(ind, ind + 1);
                            }
                        } else {
                            let ind = self
                                .slot_map_store
                                .root_workspaces
                                .iter()
                                .position(|k| *k == selected)
                                .unwrap();
                            if ind < self.slot_map_store.root_workspaces.len() - 1 {
                                self.slot_map_store.root_workspaces.swap(ind, ind + 1);
                            }
                        }
                    }
                }

                (_, KeyCode::Char('l')) => {
                    if let Some(selected) = self.slot_tree_state.selected_workspace {
                        self.slot_tree_state.ws_opened.insert(selected);
                        self.slot_tree_state.selected_todo = None;
                    }
                }

                (_, KeyCode::Char('h')) => {
                    if let Some(selected) = self.slot_tree_state.selected_workspace {
                        self.slot_tree_state.ws_opened.remove(&selected);
                        self.slot_tree_state.selected_todo = None;
                    }
                }

                (_, KeyCode::Char('i')) => {
                    if let Some(selected) = self.slot_tree_state.selected_workspace {
                        let workspace = self.slot_map_store.workspaces_map.get(selected).unwrap();
                        self.input = Input::new(workspace.description.clone());
                        self.new_editing_id = Some(selected);
                    }
                }
                (_, KeyCode::Char('a')) => {
                    let new_item = WorkspaceItem {
                        id: Uuid::new_v4().to_string(),
                        description: "".into(),
                        children: vec![],
                        todos: vec![],
                    };
                    let new_item_key = self.slot_map_store.workspaces_map.insert(new_item);

                    if let Some(selected) = self.slot_tree_state.selected_workspace {
                        // Find from rendered.
                        let parent_key = self
                            .slot_tree_state
                            .ws_tree
                            .iter()
                            .find(|w| w.key == selected)
                            .unwrap()
                            .parent;

                        if let Some(parent_key) = parent_key {
                            // Nested
                            let workspace = self
                                .slot_map_store
                                .workspaces_map
                                .get_mut(parent_key)
                                .unwrap();
                            let ind = workspace
                                .children
                                .iter()
                                .position(|k| *k == selected)
                                .unwrap();
                            workspace.children.insert(ind + 1, new_item_key);
                        } else {
                            // Top level
                            let ind = self
                                .slot_map_store
                                .root_workspaces
                                .iter()
                                .position(|k| *k == selected)
                                .unwrap();
                            self.slot_map_store
                                .root_workspaces
                                .insert(ind + 1, new_item_key);
                        }
                    } else {
                        self.slot_map_store.root_workspaces.push(new_item_key);
                    }
                    self.input = Input::new("".into());
                    self.new_editing_id = Some(new_item_key);
                    self.slot_tree_state.selected_workspace = Some(new_item_key);
                }
                (_, KeyCode::Char('A')) => {
                    if let Some(selected) = self.slot_tree_state.selected_workspace {
                        self.slot_tree_state.ws_opened.insert(selected);

                        let new_item = WorkspaceItem {
                            id: Uuid::new_v4().to_string(),
                            description: "".into(),
                            children: vec![],
                            todos: vec![],
                        };

                        let new_item_key = self.slot_map_store.workspaces_map.insert(new_item);
                        let workspace = self
                            .slot_map_store
                            .workspaces_map
                            .get_mut(selected)
                            .unwrap();
                        workspace.children.push(new_item_key);
                        self.input = Input::new("".into());
                        self.new_editing_id = Some(new_item_key);
                        self.slot_tree_state.selected_workspace = Some(new_item_key);
                    }
                }

                (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
                    if let Some(selected) = self.slot_tree_state.selected_workspace {
                        self.sorting = SortingItem::Workspace(selected)
                    }
                }

                (_, KeyCode::Char('y')) => {
                    if !self.slot_tree_state.multi_selected_workspaces.is_empty() {
                        // Copy multi-selected workspaces to clipboard
                        self.clipboard_workspaces = self
                            .slot_tree_state
                            .multi_selected_workspaces
                            .iter()
                            .cloned()
                            .collect();
                    } else if let Some(selected) = self.slot_tree_state.selected_workspace {
                        // Copy single workspace to clipboard
                        self.clipboard_workspaces = vec![selected];
                    }
                }

                (_, KeyCode::Char('p')) => {
                    if !self.clipboard_workspaces.is_empty() {
                        self.paste_multi_selected_workspaces_at_cursor();
                    }
                }

                (_, KeyCode::Char('P')) => {
                    // Note that P is only supported for single workspace paste.
                    if !self.clipboard_workspaces.is_empty() {
                        if let Some(selected) = self.slot_tree_state.selected_workspace {
                            // Paste the first workspace from clipboard as child
                            let clipboard_ws_key = self.clipboard_workspaces[0];
                            self.paste_workspace_as_child(clipboard_ws_key, selected);
                        }
                    }
                }

                (_, KeyCode::Char('x')) => {
                    if !self.slot_tree_state.multi_selected_workspaces.is_empty() {
                        self.cut_multi_selected_workspaces();
                    } else if let Some(selected) = self.slot_tree_state.selected_workspace {
                        self.clipboard_workspaces.clear();
                        self.clipboard_workspaces = vec![selected];
                        self.delete_workspace(selected);
                    }
                }

                (_, KeyCode::Char(' ')) => {
                    // Toggle multi-selection for current workspace
                    if let Some(selected) = self.slot_tree_state.selected_workspace {
                        if self
                            .slot_tree_state
                            .multi_selected_workspaces
                            .contains(&selected)
                        {
                            self.slot_tree_state
                                .multi_selected_workspaces
                                .remove(&selected);
                        } else {
                            self.slot_tree_state
                                .multi_selected_workspaces
                                .insert(selected);
                        }
                    }
                }

                _ => {}
            },
        }
    }

    fn handle_todos_key_event(&mut self, key: KeyEvent) {
        if self.search_mode {
            match key.code {
                KeyCode::Char(c) => {
                    self.search_str.push(c);
                    self.update_search_matches();
                }
                KeyCode::Backspace => {
                    self.search_str.pop();
                    self.update_search_matches();
                }
                KeyCode::Esc | KeyCode::Enter => {
                    self.search_mode = false;
                    self.search_str.clear();
                }
                _ => {}
            }
            return;
        }

        if key.code == KeyCode::Char('/') {
            self.search_mode = true;
            self.search_str.clear();
            self.search_matches.clear();
            self.current_match_index = 0;
            return;
        }

        if let SortingItem::Todo(todo_key) = self.sorting {
            match (key.modifiers, key.code) {
                // 1 is for reverse. If top todo ise selcted then reverse all
                // the todos in the workspace. Else reverse all the todos in the
                // parent todo.
                (_, KeyCode::Char('1')) => {
                    let parent_key = self
                        .slot_tree_state
                        .todo_tree
                        .iter()
                        .find(|w| w.key == todo_key)
                        .unwrap()
                        .parent;

                    if let Some(parent_key) = parent_key {
                        let mut children = self
                            .slot_map_store
                            .todos_map
                            .get(parent_key)
                            .unwrap()
                            .children
                            .clone();

                        children.reverse();

                        self.slot_map_store
                            .todos_map
                            .get_mut(parent_key)
                            .unwrap()
                            .children = children;
                    } else {
                        let workspace = self
                            .slot_map_store
                            .workspaces_map
                            .get_mut(self.slot_tree_state.selected_workspace.unwrap())
                            .unwrap();
                        workspace.todos.reverse();
                    }

                    self.sorting = SortingItem::None;
                }
                (_, KeyCode::Char(n)) => {
                    let parent_key = self
                        .slot_tree_state
                        .todo_tree
                        .iter()
                        .find(|w| w.key == todo_key)
                        .unwrap()
                        .parent;

                    if let Some(parent_key) = parent_key {
                        let mut children = self
                            .slot_map_store
                            .todos_map
                            .get(parent_key)
                            .unwrap()
                            .children
                            .clone();

                        self.sort_todos(&mut children, n);

                        self.slot_map_store
                            .todos_map
                            .get_mut(parent_key)
                            .unwrap()
                            .children = children;
                    } else {
                        let mut children = self
                            .slot_map_store
                            .workspaces_map
                            .get(self.slot_tree_state.selected_workspace.unwrap())
                            .unwrap()
                            .todos
                            .clone();

                        self.sort_todos(&mut children, n);

                        self.slot_map_store
                            .workspaces_map
                            .get_mut(self.slot_tree_state.selected_workspace.unwrap())
                            .unwrap()
                            .todos = children;
                    }

                    self.sorting = SortingItem::None;
                }
                _ => {}
            }

            return;
        }

        let new_editing_id = self.new_editing_id.clone();
        match new_editing_id {
            Some(id) => {
                match (key.modifiers, key.code) {
                    (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => self.quit(),

                    (_, KeyCode::Esc) | (_, KeyCode::Enter) => {
                        let todo = self.slot_map_store.todos_map.get_mut(id).unwrap();
                        todo.description = self.input.value().to_string();
                        self.new_editing_id = None;
                    }

                    _ => {
                        self.input.handle_event(&crossterm::event::Event::Key(key));
                    }
                };
            }
            None => match (key.modifiers, key.code) {
                (_, KeyCode::Esc | KeyCode::Char('q'))
                | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => self.quit(),

                (_, KeyCode::Tab) => self.active_screen = Screen::Workspaces,

                (_, KeyCode::Char('j')) => {
                    if let Some(selected) = self.slot_tree_state.selected_todo {
                        let index = self
                            .slot_tree_state
                            .todo_tree
                            .iter()
                            .position(|w| w.key == selected)
                            .unwrap();
                        if (index + 1) < self.slot_tree_state.todo_tree.len() {
                            self.slot_tree_state.selected_todo =
                                Some(self.slot_tree_state.todo_tree[index + 1].key);
                        }
                    } else {
                        self.slot_tree_state.selected_todo =
                            self.slot_tree_state.todo_tree.first().map(|t| t.key);
                    }
                }

                (_, KeyCode::Char('k')) => {
                    if let Some(selected) = self.slot_tree_state.selected_todo {
                        let index = self
                            .slot_tree_state
                            .todo_tree
                            .iter()
                            .position(|w| w.key == selected)
                            .unwrap();
                        if index > 0 {
                            self.slot_tree_state.selected_todo =
                                Some(self.slot_tree_state.todo_tree[index - 1].key);
                        }
                    }
                }

                (_, KeyCode::Char('l')) => {
                    if let Some(selected) = self.slot_tree_state.selected_todo {
                        self.slot_tree_state.todo_opened.insert(selected);
                    }
                }

                (_, KeyCode::Char('h')) => {
                    if let Some(selected) = self.slot_tree_state.selected_todo {
                        self.slot_tree_state.todo_opened.remove(&selected);
                    }
                }

                (_, KeyCode::Char('i')) => {
                    if let Some(selected) = self.slot_tree_state.selected_todo {
                        let todo = self.slot_map_store.todos_map.get(selected).unwrap();
                        self.input = Input::new(todo.description.clone());
                        self.new_editing_id = Some(selected);
                    }
                }
                (_, KeyCode::Char('a')) => {
                    let new_item = TodoItem {
                        id: Uuid::new_v4().to_string(),
                        description: "".into(),
                        children: vec![],
                        due: None,
                        effort: 0,
                        pending: true,
                        urgency: 0,
                    };
                    let new_item_key = self.slot_map_store.todos_map.insert(new_item);

                    if let Some(selected) = self.slot_tree_state.selected_todo {
                        // Find from rendered.
                        let parent_key = self
                            .slot_tree_state
                            .todo_tree
                            .iter()
                            .find(|w| w.key == selected)
                            .unwrap()
                            .parent;

                        if let Some(parent_key) = parent_key {
                            // Nested
                            let todo = self.slot_map_store.todos_map.get_mut(parent_key).unwrap();
                            let ind = todo.children.iter().position(|k| *k == selected).unwrap();
                            todo.children.insert(ind + 1, new_item_key);
                        } else {
                            // Top level
                            let workspace = self
                                .slot_map_store
                                .workspaces_map
                                .get_mut(self.slot_tree_state.selected_workspace.unwrap())
                                .unwrap();

                            let ind = workspace.todos.iter().position(|k| *k == selected).unwrap();
                            workspace.todos.insert(ind + 1, new_item_key);
                        }
                    } else {
                        let workspace = self
                            .slot_map_store
                            .workspaces_map
                            .get_mut(self.slot_tree_state.selected_workspace.unwrap())
                            .unwrap();

                        workspace.todos.push(new_item_key);
                    }
                    self.input = Input::new("".into());
                    self.new_editing_id = Some(new_item_key);
                    self.slot_tree_state.selected_todo = Some(new_item_key);
                }
                (_, KeyCode::Char('A')) => {
                    if let Some(selected) = self.slot_tree_state.selected_todo {
                        self.slot_tree_state.todo_opened.insert(selected);

                        let new_item = TodoItem {
                            id: Uuid::new_v4().to_string(),
                            description: "".into(),
                            children: vec![],
                            due: None,
                            effort: 0,
                            pending: true,
                            urgency: 0,
                        };

                        let new_item_key = self.slot_map_store.todos_map.insert(new_item);

                        let todo = self.slot_map_store.todos_map.get_mut(selected).unwrap();

                        todo.children.push(new_item_key);
                        self.input = Input::new("".into());
                        self.new_editing_id = Some(new_item_key);
                        self.slot_tree_state.selected_todo = Some(new_item_key);
                    }
                }
                (_, KeyCode::Char('c')) => {
                    if let Some(selected) = self.slot_tree_state.selected_todo {
                        let todo = self.slot_map_store.todos_map.get_mut(selected).unwrap();
                        todo.pending = !todo.pending;
                    }
                }
                (_, KeyCode::Char('+')) => {
                    if let Some(selected) = self.slot_tree_state.selected_todo {
                        let todo = self.slot_map_store.todos_map.get_mut(selected).unwrap();
                        if todo.urgency < 3 {
                            todo.urgency += 1;
                        }
                    }
                }
                (_, KeyCode::Char('_')) => {
                    if let Some(selected) = self.slot_tree_state.selected_todo {
                        let todo = self.slot_map_store.todos_map.get_mut(selected).unwrap();
                        if todo.urgency > 0 {
                            todo.urgency -= 1;
                        }
                    }
                }
                (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
                    if let Some(selected) = self.slot_tree_state.selected_todo {
                        self.sorting = SortingItem::Todo(selected)
                    }
                }
                (_, KeyCode::Char('y')) => {
                    if !self.slot_tree_state.multi_selected_todos.is_empty() {
                        // Copy multi-selected todos to clipboard
                        self.clipboard_todos = self
                            .slot_tree_state
                            .multi_selected_todos
                            .iter()
                            .cloned()
                            .collect();
                    } else if let Some(selected) = self.slot_tree_state.selected_todo {
                        // Copy single todo to clipboard
                        self.clipboard_todos = vec![selected];
                    }
                }

                (_, KeyCode::Char('p')) => {
                    if !self.clipboard_todos.is_empty() {
                        self.paste_multi_selected_todos_at_cursor();
                    }
                }

                (_, KeyCode::Char('P')) => {
                    if !self.clipboard_todos.is_empty() {
                        if let Some(selected) = self.slot_tree_state.selected_todo {
                            // Paste the first todo from clipboard as child
                            let clipboard_key = self.clipboard_todos[0];
                            self.paste_todo_as_child(clipboard_key, selected);
                        }
                    }
                }

                (_, KeyCode::Char('x')) => {
                    if !self.slot_tree_state.multi_selected_todos.is_empty() {
                        self.cut_multi_selected_todos();
                    } else if let Some(selected) = self.slot_tree_state.selected_todo {
                        self.clipboard_todos.clear();
                        self.clipboard_todos.push(selected);
                        self.delete_todo(selected);
                    }
                }

                (_, KeyCode::Char('K')) => {
                    if let Some(selected) = self.slot_tree_state.selected_todo {
                        let parent = self
                            .slot_tree_state
                            .todo_tree
                            .iter()
                            .find(|w| w.key == selected)
                            .unwrap()
                            .parent;

                        if let Some(parent_key) = parent {
                            let parent = self.slot_map_store.todos_map.get_mut(parent_key).unwrap();
                            let ind = parent.children.iter().position(|k| *k == selected).unwrap();

                            if ind > 0 {
                                parent.children.swap(ind, ind - 1);
                            }
                        } else {
                            let workspace = self
                                .slot_map_store
                                .workspaces_map
                                .get_mut(self.slot_tree_state.selected_workspace.unwrap())
                                .unwrap();

                            let ind = workspace.todos.iter().position(|k| *k == selected).unwrap();

                            if ind > 0 {
                                workspace.todos.swap(ind, ind - 1);
                            }
                        }
                    }
                }

                (_, KeyCode::Char('J')) => {
                    if let Some(selected) = self.slot_tree_state.selected_todo {
                        let parent = self
                            .slot_tree_state
                            .todo_tree
                            .iter()
                            .find(|w| w.key == selected)
                            .unwrap()
                            .parent;

                        if let Some(parent_key) = parent {
                            let parent = self.slot_map_store.todos_map.get_mut(parent_key).unwrap();
                            let ind = parent.children.iter().position(|k| *k == selected).unwrap();

                            if ind < parent.children.len() - 1 {
                                parent.children.swap(ind, ind + 1);
                            }
                        } else {
                            let workspace = self
                                .slot_map_store
                                .workspaces_map
                                .get_mut(self.slot_tree_state.selected_workspace.unwrap())
                                .unwrap();

                            let ind = workspace.todos.iter().position(|k| *k == selected).unwrap();

                            if ind < workspace.todos.len() - 1 {
                                workspace.todos.swap(ind, ind + 1);
                            }
                        }
                    }
                }

                (_, KeyCode::Char(' ')) => {
                    // Toggle multi-selection for current todo
                    if let Some(selected) = self.slot_tree_state.selected_todo {
                        if self
                            .slot_tree_state
                            .multi_selected_todos
                            .contains(&selected)
                        {
                            self.slot_tree_state.multi_selected_todos.remove(&selected);
                        } else {
                            self.slot_tree_state.multi_selected_todos.insert(selected);
                        }
                    }
                }

                (_, KeyCode::Char('n')) => {
                    if !self.search_matches.is_empty() {
                        self.current_match_index =
                            (self.current_match_index + 1) % self.search_matches.len();

                        // Select the todo if it's in the tree
                        if let Some(_) = self
                            .slot_tree_state
                            .todo_tree
                            .iter()
                            .find(|t| t.key == self.search_matches[self.current_match_index])
                        {
                            self.slot_tree_state.selected_todo =
                                Some(self.search_matches[self.current_match_index]);
                        }
                    }
                }
                _ => {}
            },
        }
    }

    /// Reads the crossterm events and updates the state of [`App`].
    ///
    /// If your application needs to perform work in between handling events, you can use the
    /// [`event::poll`] function to check if there are any events available with a timeout.
    fn handle_crossterm_events(&mut self, event: crossterm::event::Event) -> Result<()> {
        match event {
            // it's important to check KeyEventKind::Press to avoid handling key release events
            Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key_event(key),
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
            _ => {}
        }
        Ok(())
    }

    /// Handles the key events and updates the state of [`App`].
    fn on_key_event(&mut self, key: KeyEvent) {
        match self.active_screen {
            Screen::Workspaces => {
                self.handle_workspace_key_event(key);
            }
            Screen::Todos => {
                self.handle_todos_key_event(key);
            }
        }
    }

    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.running = false;
    }

    // FIXME: YOU can use references here for tree. Perfomance

    // Multi-selection helper methods
    fn cut_multi_selected_todos(&mut self) {
        if self.slot_tree_state.multi_selected_todos.is_empty() {
            return;
        }

        // Clear existing clipboard before copying selected todos
        self.clipboard_todos.clear();
        self.clipboard_todos = self
            .slot_tree_state
            .multi_selected_todos
            .iter()
            .cloned()
            .collect();

        // Delete all selected todos without updating selection state
        self.clipboard_todos.clone().iter().for_each(|key| {
            self.delete_todo(*key);
        });

        // Clear multi-selection state
        self.slot_tree_state.multi_selected_todos.clear();
    }

    fn cut_multi_selected_workspaces(&mut self) {
        if self.slot_tree_state.multi_selected_workspaces.is_empty() {
            return;
        }

        // Clear existing clipboard before copying selected workspaces
        self.clipboard_workspaces.clear();
        self.clipboard_workspaces = self
            .slot_tree_state
            .multi_selected_workspaces
            .iter()
            .cloned()
            .collect();

        self.clipboard_workspaces.clone().iter().for_each(|key| {
            self.delete_workspace(*key);
        });

        // Clear multi-selection state
        self.slot_tree_state.multi_selected_workspaces.clear();
    }

    fn paste_multi_selected_todos_at_cursor(&mut self) {
        if self.clipboard_todos.is_empty() {
            return;
        }

        // First, collect all todo keys to clone
        let todo_keys_to_clone: Vec<DefaultKey> = self.clipboard_todos.clone();

        // Clone todos first to avoid borrowing issues
        let mut cloned_todos = Vec::new();
        for todo_key in todo_keys_to_clone {
            cloned_todos.push(self.clone_todo(todo_key));
        }

        if let Some(selected) = self.slot_tree_state.selected_todo {
            // Collect tree info before making mutations
            let parent_key = self
                .slot_tree_state
                .todo_tree
                .iter()
                .find(|w| w.key == selected)
                .map(|item| item.parent);

            if let Some(Some(parent)) = parent_key {
                // Find insertion point in parent's children
                let insertion_point = self
                    .slot_map_store
                    .todos_map
                    .get(parent)
                    .unwrap()
                    .children
                    .iter()
                    .position(|w| w == &selected)
                    .unwrap()
                    + 1;

                // Insert all cloned todos into parent's children
                let parent_todo = self.slot_map_store.todos_map.get_mut(parent).unwrap();
                for (index, new_todo_key) in cloned_todos.iter().enumerate() {
                    parent_todo
                        .children
                        .insert(insertion_point + index, *new_todo_key);
                }
            } else {
                // Selected todo is at workspace level
                let workspace_key = self.slot_tree_state.selected_workspace.unwrap();
                let insertion_point = self
                    .slot_map_store
                    .workspaces_map
                    .get(workspace_key)
                    .unwrap()
                    .todos
                    .iter()
                    .position(|w| w == &selected)
                    .unwrap()
                    + 1;

                // Insert all cloned todos into workspace
                let workspace = self
                    .slot_map_store
                    .workspaces_map
                    .get_mut(workspace_key)
                    .unwrap();
                for (index, new_todo_key) in cloned_todos.iter().enumerate() {
                    workspace
                        .todos
                        .insert(insertion_point + index, *new_todo_key);
                }
            }
        } else {
            // No cursor position, paste at end of workspace
            let workspace_key = self.slot_tree_state.selected_workspace.unwrap();
            let workspace = self
                .slot_map_store
                .workspaces_map
                .get_mut(workspace_key)
                .unwrap();
            for new_todo_key in cloned_todos {
                workspace.todos.push(new_todo_key);
            }
        }

        self.slot_tree_state.multi_selected_todos.clear();

        // Update tree state
        self.slot_tree_state
            .update_workspace_tree_state(&self.slot_map_store);
    }

    fn paste_multi_selected_workspaces_at_cursor(&mut self) {
        if self.clipboard_workspaces.is_empty() {
            return;
        }

        // First, clone all workspaces to avoid borrowing issues
        let workspace_keys_to_clone: Vec<DefaultKey> = self.clipboard_workspaces.clone();
        let mut cloned_workspaces = Vec::new();
        for workspace_key in workspace_keys_to_clone {
            cloned_workspaces.push(self.clone_workspace(workspace_key));
        }

        if let Some(selected) = self.slot_tree_state.selected_workspace {
            // Collect parent info first
            let parent_key = self
                .slot_tree_state
                .ws_tree
                .iter()
                .find(|w| w.key == selected)
                .map(|workspace| workspace.parent);

            if let Some(Some(parent)) = parent_key {
                // Find insertion point in parent's children
                let insertion_point = self
                    .slot_map_store
                    .workspaces_map
                    .get(parent)
                    .unwrap()
                    .children
                    .iter()
                    .position(|w| w == &selected)
                    .unwrap()
                    + 1;

                // Insert all cloned workspaces
                let parent_workspace = self.slot_map_store.workspaces_map.get_mut(parent).unwrap();
                for (index, new_workspace_key) in cloned_workspaces.iter().enumerate() {
                    parent_workspace
                        .children
                        .insert(insertion_point + index, *new_workspace_key);
                }
            } else {
                // Selected workspace is at root level
                let insertion_point = self
                    .slot_map_store
                    .root_workspaces
                    .iter()
                    .position(|w| w == &selected)
                    .unwrap()
                    + 1;

                // Insert all cloned workspaces
                for (index, new_workspace_key) in cloned_workspaces.iter().enumerate() {
                    self.slot_map_store
                        .root_workspaces
                        .insert(insertion_point + index, *new_workspace_key);
                }
            }
        } else {
            // No cursor position, paste at end of root
            for new_workspace_key in cloned_workspaces {
                self.slot_map_store.root_workspaces.push(new_workspace_key);
            }
        }

        self.slot_tree_state.multi_selected_workspaces.clear();

        // Update tree state
        self.slot_tree_state
            .update_workspace_tree_state(&self.slot_map_store);
    }

    fn clear_multi_selection_when_workspace_changes(&mut self) {
        self.slot_tree_state.multi_selected_todos.clear();
    }
}

fn get_crossterm_events(tx: mpsc::Sender<crossterm::event::Event>) -> Result<()> {
    loop {
        let event = event::read()?;
        tx.send(event).unwrap();
    }
}

#[derive(Default)]
struct ActiveTree {
    key: DefaultKey,
    parent: Option<DefaultKey>,
    depth: usize,
}

#[derive(Default)]
struct SlotTreeState {
    pub selected_todo: Option<DefaultKey>,
    pub selected_workspace: Option<DefaultKey>,
    pub ws_opened: HashSet<DefaultKey>,
    pub todo_opened: HashSet<DefaultKey>,
    pub ws_tree: Vec<ActiveTree>,
    pub todo_tree: Vec<ActiveTree>,
    pub multi_selected_todos: HashSet<DefaultKey>,
    pub multi_selected_workspaces: HashSet<DefaultKey>,
}

impl SlotTreeState {
    fn add_workspace_to_tree(
        &self,
        ws_tree: &mut Vec<ActiveTree>,
        store: &SlotMapStore,
        key: DefaultKey,
        depth: usize,
        parent: Option<DefaultKey>,
    ) {
        ws_tree.push(ActiveTree {
            key: key,
            parent: parent,
            depth,
        });

        if self.ws_opened.contains(&key) {
            let workspace = store.workspaces_map.get(key).unwrap();
            workspace.children.iter().for_each(|k| {
                self.add_workspace_to_tree(ws_tree, store, *k, depth + 1, Some(key));
            });
        };
    }

    fn add_todo_to_tree(
        &self,
        todo_tree: &mut Vec<ActiveTree>,
        store: &SlotMapStore,
        key: DefaultKey,
        depth: usize,
        parent: Option<DefaultKey>,
    ) {
        todo_tree.push(ActiveTree {
            key: key,
            parent: parent,
            depth,
        });

        if self.todo_opened.contains(&key) {
            let todo = store.todos_map.get(key).unwrap();
            todo.children.iter().for_each(|k| {
                self.add_todo_to_tree(todo_tree, store, *k, depth + 1, Some(key));
            });
        }
    }

    pub fn update_workspace_tree_state(&mut self, store: &store::SlotMapStore) {
        let mut ws_tree = Vec::new();
        store.root_workspaces.iter().for_each(|w| {
            self.add_workspace_to_tree(&mut ws_tree, store, *w, 0, None);
        });

        let mut todo_tree = Vec::new();
        if let Some(selected) = self.selected_workspace {
            let workspace = store.workspaces_map.get(selected).unwrap();
            workspace.todos.iter().for_each(|t| {
                self.add_todo_to_tree(&mut todo_tree, store, *t, 0, None);
            });
        }

        self.ws_tree = ws_tree;
        self.todo_tree = todo_tree;
    }
}

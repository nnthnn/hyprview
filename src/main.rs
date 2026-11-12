mod cli;
mod config;
mod icons;
mod ipc;
mod layout;
mod render;

use config::Config;
use ipc::WorkspaceInfo;
use render::Thumbnail;

use std::collections::VecDeque;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};

use wayland_client::{
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_pointer, wl_registry, wl_seat, wl_shm,
        wl_shm_pool, wl_surface,
    },
    Connection, Dispatch, EventQueue, QueueHandle, WEnum,
};
use wayland_cursor::CursorTheme;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

// Generated Hyprland protocol bindings
#[allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
#[allow(non_upper_case_globals, non_snake_case, unused_imports, missing_docs)]
#[allow(clippy::all)]
pub mod hyprland_toplevel_export {
    use wayland_client;
    use wayland_client::protocol::*;
    use wayland_protocols_wlr::foreign_toplevel::v1::client::*;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        use wayland_protocols_wlr::foreign_toplevel::v1::client::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/hyprland-toplevel-export-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/hyprland-toplevel-export-v1.xml");
}
use hyprland_toplevel_export::{
    hyprland_toplevel_export_frame_v1, hyprland_toplevel_export_manager_v1,
};
use hyprland_toplevel_export_frame_v1::HyprlandToplevelExportFrameV1;
use hyprland_toplevel_export_manager_v1::HyprlandToplevelExportManagerV1;

// ── signal handling ─────────────────────────────────────────────────────────

static G_QUIT: AtomicBool = AtomicBool::new(false);
static G_TOGGLE: AtomicBool = AtomicBool::new(false);
static G_RELOAD: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn sig_handler(sig: libc::c_int) {
    if sig == libc::SIGUSR1 {
        G_TOGGLE.store(true, Ordering::Relaxed);
    } else if sig == libc::SIGHUP {
        G_RELOAD.store(true, Ordering::Relaxed);
    } else {
        G_QUIT.store(true, Ordering::Relaxed);
    }
}

// ── key mapping (evdev keycodes → XKB keysyms) ──────────────────────────────

const XKB_KEY_ESCAPE: u32 = 0xff1b;
const XKB_KEY_RETURN: u32 = 0xff0d;
const XKB_KEY_BACKSPACE: u32 = 0xff08;
const XKB_KEY_LEFT: u32 = 0xff51;
const XKB_KEY_UP: u32 = 0xff52;
const XKB_KEY_RIGHT: u32 = 0xff53;
const XKB_KEY_DOWN: u32 = 0xff54;
const XKB_KEY_DELETE: u32 = 0xffff;
const XKB_KEY_TAB: u32 = 0xff09;
const XKB_KEY_1: u32 = 0x0031;
const XKB_KEY_9: u32 = 0x0039;

const KEYCODE_LEFTSHIFT: u32 = 42;
const KEYCODE_RIGHTSHIFT: u32 = 54;

fn key_name_to_keysym(name: &str) -> Option<u32> {
    match name {
        "Escape" => Some(XKB_KEY_ESCAPE),
        "Return" | "Enter" => Some(XKB_KEY_RETURN),
        "Left" => Some(XKB_KEY_LEFT),
        "Up" => Some(XKB_KEY_UP),
        "Right" => Some(XKB_KEY_RIGHT),
        "Down" => Some(XKB_KEY_DOWN),
        "Tab" => Some(XKB_KEY_TAB),
        "space" | "Space" => Some(0x0020),
        "Delete" => Some(XKB_KEY_DELETE),
        _ if name.len() == 1 => {
            let c = name.chars().next()?;
            if c.is_ascii_graphic() {
                Some(c as u32)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn resolve_keys(names: &[String]) -> Vec<u32> {
    names.iter().filter_map(|n| key_name_to_keysym(n)).collect()
}

struct ResolvedKeys {
    close: Vec<u32>,
    confirm: Vec<u32>,
    left: Vec<u32>,
    right: Vec<u32>,
    up: Vec<u32>,
    down: Vec<u32>,
    move_window: Vec<u32>,
    close_window: Vec<u32>,
    tab: Vec<u32>,
}

impl ResolvedKeys {
    fn from_config(kb: &config::KeybindingsConfig) -> Self {
        Self {
            close: resolve_keys(&kb.close),
            confirm: resolve_keys(&kb.confirm),
            left: resolve_keys(&kb.left),
            right: resolve_keys(&kb.right),
            up: resolve_keys(&kb.up),
            down: resolve_keys(&kb.down),
            move_window: resolve_keys(&kb.move_window),
            close_window: resolve_keys(&kb.close_window),
            tab: resolve_keys(&kb.tab),
        }
    }
}

fn keycode_to_keysym(keycode: u32) -> u32 {
    // Compositors send raw evdev keycodes via wl_keyboard.
    // Letter keysyms equal their ASCII value.
    match keycode {
        1 => XKB_KEY_ESCAPE,
        14 => XKB_KEY_BACKSPACE,
        15 => XKB_KEY_TAB,
        28 => XKB_KEY_RETURN,
        57 => 0x0020, // space
        103 => XKB_KEY_UP,
        105 => XKB_KEY_LEFT,
        106 => XKB_KEY_RIGHT,
        108 => XKB_KEY_DOWN,
        // digits 1-9 (evdev 2-10) and 0 (11); - = (12-13)
        2 => 0x31,
        3 => 0x32,
        4 => 0x33,
        5 => 0x34,
        6 => 0x35,
        7 => 0x36,
        8 => 0x37,
        9 => 0x38,
        10 => 0x39,
        11 => 0x30,
        12 => 0x2d,
        13 => 0x3d,
        // top row: q w e r t y u i o p [ ]
        16 => 0x71,
        17 => 0x77,
        18 => 0x65,
        19 => 0x72,
        20 => 0x74,
        21 => 0x79,
        22 => 0x75,
        23 => 0x69,
        24 => 0x6f,
        25 => 0x70,
        26 => 0x5b,
        27 => 0x5d,
        // home row: a s d f g h j k l ; ' `
        30 => 0x61,
        31 => 0x73,
        32 => 0x64,
        33 => 0x66,
        34 => 0x67,
        35 => 0x68,
        36 => 0x6a,
        37 => 0x6b,
        38 => 0x6c,
        39 => 0x3b,
        40 => 0x27,
        41 => 0x60,
        // bottom row: \ z x c v b n m , . /
        43 => 0x5c,
        44 => 0x7a,
        45 => 0x78,
        46 => 0x63,
        47 => 0x76,
        48 => 0x62,
        49 => 0x6e,
        50 => 0x6d,
        51 => 0x2c,
        52 => 0x2e,
        53 => 0x2f,
        111 => XKB_KEY_DELETE,
        _ => 0,
    }
}

// ── capture frame state ──────────────────────────────────────────────────────

#[derive(Default)]
struct CaptureFrameData {
    format: u32,
    width: u32,
    height: u32,
    stride: u32,
    buffer_done: bool,
    frame_ready: bool,
    frame_failed: bool,
}

enum CapturePhase {
    Phase1,
    Phase2 {
        wl_buf: wl_buffer::WlBuffer,
        mmap_ptr: *mut libc::c_void,
        size: usize,
        width: u32,
        height: u32,
        stride: u32,
    },
}

struct ActiveCapture {
    frame: HyprlandToplevelExportFrameV1,
    address: u64,
    phase: CapturePhase,
    started: std::time::Instant,
}

// ── application state ────────────────────────────────────────────────────────

struct AppState {
    // Wayland globals
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    seat: Option<wl_seat::WlSeat>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    toplevel_export: Option<HyprlandToplevelExportManagerV1>,

    // Per-show Wayland objects (recreated on each show)
    wl_surf: Option<wl_surface::WlSurface>,
    layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    wl_buf: Option<wl_buffer::WlBuffer>,

    // Surface / overlay state
    width: u32,
    height: u32,
    configured: bool,
    visible: bool,
    should_close: bool,
    needs_redraw: bool,

    // App state
    workspaces: Vec<WorkspaceInfo>,
    thumbnails: Vec<Thumbnail>,
    icon_cache: icons::IconCache,
    selected: usize,
    no_preview: bool,
    active_window_address: u64,
    config: Config,
    keys: ResolvedKeys,
    // CLI overrides are remembered so config reloads can re-merge them.
    cli_no_preview: bool,
    cli_allow_mouse: bool,

    // Type-to-filter
    filter: String,
    filter_match: Option<u64>,

    // Tab-cycle
    shift_held: bool,
    tab_target: Option<u64>,

    // Mouse support (only active when --allow-mouse is set).
    allow_mouse: bool,
    pointer: Option<wl_pointer::WlPointer>,
    pointer_x: f64,
    pointer_y: f64,
    hovered_window: Option<u64>,
    cursor_surface: Option<wl_surface::WlSurface>,
    cursor_theme: Option<CursorTheme>,
    cursor_enter_serial: u32,
    cursor_is_pointer: bool,

    // Async thumbnail capture
    pending_capture: Option<CaptureFrameData>,
    capture_queue: VecDeque<u64>,
    active_capture: Option<ActiveCapture>,

    // Window to focus after the overlay surface is fully destroyed: (workspace_id, address, cx, cy)
    pending_focus: Option<(i32, u64, i32, i32)>,
}

fn find_single_window_match(workspaces: &[WorkspaceInfo], filter: &str) -> Option<u64> {
    let f = filter.to_lowercase();
    let mut found = None;
    for ws in workspaces {
        for c in &ws.clients {
            if c.class_name.to_lowercase().contains(&f) || c.title.to_lowercase().contains(&f) {
                if found.is_some() {
                    return None;
                } // more than one
                found = Some(c.address);
            }
        }
    }
    found
}

fn workspace_matches(ws: &WorkspaceInfo, filter: &str) -> bool {
    ws.name.to_lowercase().contains(filter)
        || ws.clients.iter().any(|c| {
            c.class_name.to_lowercase().contains(filter) || c.title.to_lowercase().contains(filter)
        })
}

fn tab_candidates(workspaces: &[WorkspaceInfo], filter: &str) -> Vec<u64> {
    let f = filter.to_lowercase();
    workspaces
        .iter()
        .flat_map(|ws| {
            // ws.clients arrives in Hyprland's internal list order, not screen
            // position, so sort into reading order (top-to-bottom, left-to-right)
            // to match what the grid actually shows.
            let mut clients: Vec<&ipc::ClientInfo> = ws.clients.iter().collect();
            clients.sort_by_key(|c| (c.y, c.x));
            clients
        })
        .filter(|c| {
            f.is_empty()
                || c.class_name.to_lowercase().contains(&f)
                || c.title.to_lowercase().contains(&f)
        })
        .map(|c| c.address)
        .collect()
}

/// Address to highlight on the next Tab press.
///
/// `current` is the window the cycle is anchored to: the current Tab target,
/// or (before the first Tab) the focused window, so cycling starts next to it
/// rather than at the top of the grid. Falls back to the ends of the list when
/// `current` is not among the candidates (e.g. it was filtered out).
fn next_tab_target(candidates: &[u64], current: u64, reverse: bool) -> Option<u64> {
    if candidates.is_empty() {
        return None;
    }
    let cur = candidates.iter().position(|&a| a == current);
    let next_index = match (cur, reverse) {
        (Some(i), false) => (i + 1) % candidates.len(),
        (Some(i), true) => {
            if i == 0 {
                candidates.len() - 1
            } else {
                i - 1
            }
        }
        (None, false) => 0,
        (None, true) => candidates.len() - 1,
    };
    Some(candidates[next_index])
}

impl AppState {
    fn new(cli_no_preview: bool, cli_allow_mouse: bool, config: Config) -> Self {
        // CLI flags override config values (OR semantics: flag=true forces on)
        let no_preview = cli_no_preview || config.behavior.no_preview;
        let allow_mouse = cli_allow_mouse || config.behavior.allow_mouse;
        Self {
            compositor: None,
            shm: None,
            seat: None,
            layer_shell: None,
            toplevel_export: None,
            wl_surf: None,
            layer_surface: None,
            wl_buf: None,
            width: 0,
            height: 0,
            configured: false,
            visible: false,
            should_close: false,
            needs_redraw: false,
            workspaces: Vec::new(),
            thumbnails: Vec::new(),
            icon_cache: icons::IconCache::new(),
            selected: 0,
            no_preview,
            active_window_address: 0,
            keys: ResolvedKeys::from_config(&config.keybindings),
            config,
            cli_no_preview,
            cli_allow_mouse,
            filter: String::new(),
            filter_match: None,
            shift_held: false,
            tab_target: None,
            allow_mouse,
            pointer: None,
            pointer_x: 0.0,
            pointer_y: 0.0,
            hovered_window: None,
            cursor_surface: None,
            cursor_theme: None,
            cursor_enter_serial: 0,
            cursor_is_pointer: false,
            pending_capture: None,
            capture_queue: VecDeque::new(),
            active_capture: None,
            pending_focus: None,
        }
    }

    /// Indices into `workspaces` that the grid actually shows. `selected` and
    /// all pointer hit-testing index into this list, not the raw workspace list,
    /// so empty workspaces being hidden cannot shift the selection highlight.
    fn visible_workspace_indices(&self) -> Vec<usize> {
        self.workspaces
            .iter()
            .enumerate()
            .filter_map(|(i, ws)| {
                (self.config.behavior.show_empty_workspaces || !ws.clients.is_empty()).then_some(i)
            })
            .collect()
    }

    fn visible_workspace_count(&self) -> usize {
        self.workspaces
            .iter()
            .filter(|ws| self.config.behavior.show_empty_workspaces || !ws.clients.is_empty())
            .count()
    }

    fn clamp_selected(&mut self) {
        let n = self.visible_workspace_count();
        if n == 0 {
            self.selected = 0;
        } else if self.selected >= n {
            self.selected = n - 1;
        }
    }

    fn selected_workspace(&self) -> Option<&WorkspaceInfo> {
        self.visible_workspace_indices()
            .get(self.selected)
            .map(|&i| &self.workspaces[i])
    }

    /// Keep the selection on the same named workspace when the visible list
    /// changes (e.g. `show_empty_workspaces` is toggled by a config reload).
    fn select_workspace_by_name(&mut self, name: Option<&str>) {
        if let Some(name) = name {
            let visible = self.visible_workspace_indices();
            if let Some(i) = visible
                .iter()
                .position(|&i| self.workspaces[i].name == name)
            {
                self.selected = i;
            }
        }
        self.clamp_selected();
    }

    fn grid_cols(&self, n: usize) -> usize {
        layout::workspace_grid_cols(n, self.config.appearance.columns)
    }

    fn init_cursor(&mut self, conn: &Connection, qh: &QueueHandle<Self>) {
        if self.cursor_surface.is_some() {
            return;
        }
        let compositor = self.compositor.clone();
        let shm = self.shm.clone();
        if let (Some(compositor), Some(shm)) = (compositor, shm) {
            self.cursor_surface = Some(compositor.create_surface(qh, ()));
            self.cursor_theme = CursorTheme::load(conn, shm, 24).ok();
        }
    }

    fn apply_cursor(&mut self, pointer: &wl_pointer::WlPointer, is_pointer: bool) {
        let surf = match self.cursor_surface.clone() {
            Some(s) => s,
            None => return,
        };
        let serial = self.cursor_enter_serial;
        let theme = match self.cursor_theme.as_mut() {
            Some(t) => t,
            None => return,
        };
        let fallbacks: &[&str] = if is_pointer {
            &["pointer", "hand2"]
        } else {
            &["default", "left_ptr"]
        };
        let mut found = false;
        for name in fallbacks {
            if let Some(cursor) = theme.get_cursor(name) {
                let image = &cursor[0];
                let (hx, hy) = image.hotspot();
                surf.attach(Some(&**image), 0, 0);
                surf.damage_buffer(0, 0, i32::MAX, i32::MAX);
                surf.commit();
                pointer.set_cursor(serial, Some(&surf), hx as i32, hy as i32);
                found = true;
                break;
            }
        }
        if !found {
            pointer.set_cursor(serial, None, 0, 0);
        }
    }

    /// Returns true if the overlay should be closed.
    fn handle_key(&mut self, keysym: u32) -> bool {
        let visible = self.visible_workspace_indices();
        let n = visible.len();
        if n == 0 {
            return true;
        }
        let cols = self.grid_cols(n);
        let k = keysym;

        // Backspace: pop filter char
        if k == XKB_KEY_BACKSPACE {
            if self.filter.pop().is_some() {
                self.update_filter_state();
                self.needs_redraw = true;
            }
            return false;
        }

        // Escape: clear Tab-highlight first, then filter, else close
        if self.keys.close.contains(&k) {
            if self.tab_target.is_some() {
                self.tab_target = None;
                self.needs_redraw = true;
                return false;
            }
            if !self.filter.is_empty() {
                self.filter.clear();
                self.filter_match = None;
                self.needs_redraw = true;
                return false;
            }
            return true;
        }

        // Confirm: Tab-highlighted window first, else single filter match, else activate workspace
        if self.keys.confirm.contains(&k) {
            // A Tab target can go stale while a filter is active (for example
            // the filtered workspace/window was closed). Ignore it rather than
            // focusing something the user can no longer see.
            let tab_target = self.tab_target.filter(|&addr| {
                self.filter.is_empty()
                    || tab_candidates(&self.workspaces, &self.filter).contains(&addr)
            });
            let target_addr = tab_target.or(self.filter_match);
            if let Some(addr) = target_addr {
                let client = self
                    .workspaces
                    .iter()
                    .flat_map(|ws| ws.clients.iter())
                    .find(|c| c.address == addr);
                if let Some(c) = client {
                    self.pending_focus = Some((c.workspace_id, addr, c.x + c.w / 2, c.y + c.h / 2));
                    return true;
                }
            }
            if !self.filter.is_empty() {
                let f = self.filter.to_lowercase();
                if let Some(i) = visible
                    .iter()
                    .position(|&wi| workspace_matches(&self.workspaces[wi], &f))
                {
                    self.selected = i;
                }
            }
            self.activate_selected_workspace();
            return true;
        }

        // Number keys: direct jump (only without filter). Out-of-range digits
        // are a no-op instead of closing the overlay.
        if self.filter.is_empty() && (XKB_KEY_1..=XKB_KEY_9).contains(&k) {
            let idx = (k - XKB_KEY_1) as usize;
            if let Some(&wi) = visible.get(idx) {
                ipc::switch_workspace(&self.workspaces[wi]);
                return true;
            }
            return false;
        }

        // Tab / Shift+Tab: cycle the highlighted window across all workspaces
        // (or across filter-matching windows, if a filter is active). Before the
        // first Tab, the cycle is anchored to the focused window rather than the
        // top of the grid.
        if self.keys.tab.contains(&k) {
            let candidates = tab_candidates(&self.workspaces, &self.filter);
            let current = self.tab_target.unwrap_or(self.active_window_address);
            if let Some(next) = next_tab_target(&candidates, current, self.shift_held) {
                self.tab_target = Some(next);
                self.needs_redraw = true;
            }
            return false;
        }

        // Arrow keys always navigate; hjkl only when filter is empty
        let is_arrow = matches!(k, XKB_KEY_LEFT | XKB_KEY_RIGHT | XKB_KEY_UP | XKB_KEY_DOWN);
        if is_arrow || self.filter.is_empty() {
            let nav_key = self.keys.right.contains(&k)
                || self.keys.left.contains(&k)
                || self.keys.down.contains(&k)
                || self.keys.up.contains(&k);
            // Any navigation press cancels the Tab highlight, even when the
            // selection can't actually move (already at an edge).
            if nav_key && self.tab_target.is_some() {
                self.tab_target = None;
                self.needs_redraw = true;
            }
            if self.keys.right.contains(&k) && self.selected + 1 < n {
                self.selected += 1;
                self.needs_redraw = true;
                return false;
            }
            if self.keys.left.contains(&k) && self.selected > 0 {
                self.selected -= 1;
                self.needs_redraw = true;
                return false;
            }
            if self.keys.down.contains(&k) && self.selected + cols < n {
                self.selected += cols;
                self.needs_redraw = true;
                return false;
            }
            if self.keys.up.contains(&k) && self.selected >= cols {
                self.selected -= cols;
                self.needs_redraw = true;
                return false;
            }
        }

        // Move window (only without filter)
        if self.filter.is_empty()
            && self.keys.move_window.contains(&k)
            && self.move_active_window_to_selected()
        {
            return true;
        }

        // Close window: target hovered window first, then single filter match
        if self.keys.close_window.contains(&k) {
            if let Some(addr) = self.hovered_window.or(self.filter_match) {
                self.close_window(addr);
            }
            return false;
        }

        // Printable char: append to filter
        if let Some(c) = char::from_u32(k).filter(|c| c.is_ascii_graphic() || *c == ' ') {
            self.filter.push(c);
            self.update_filter_state();
            self.needs_redraw = true;
        }

        false
    }

    fn update_filter_state(&mut self) {
        let f = self.filter.to_lowercase();
        self.filter_match = if f.is_empty() {
            None
        } else {
            find_single_window_match(&self.workspaces, &f)
        };
        // Only reposition the selection for a non-empty filter. Clearing the
        // filter should leave the user where they were rather than jumping
        // back to the first card.
        if !f.is_empty() {
            let visible = self.visible_workspace_indices();
            if let Some(i) = visible
                .iter()
                .position(|&wi| workspace_matches(&self.workspaces[wi], &f))
            {
                self.selected = i;
            }
        }
        // Drop a Tab highlight that the new filter excludes.
        if let Some(addr) = self.tab_target {
            if !tab_candidates(&self.workspaces, &self.filter).contains(&addr) {
                self.tab_target = None;
            }
        }
        self.clamp_selected();
    }

    fn activate_selected_workspace(&self) {
        if let Some(ws) = self.selected_workspace() {
            ipc::switch_workspace(ws);
        }
    }

    /// Move the active window to the selected workspace. Returns true on success
    /// (overlay should close).
    fn move_active_window_to_selected(&self) -> bool {
        if self.active_window_address == 0 {
            return false;
        }
        let Some(target) = self.selected_workspace() else {
            return false;
        };
        ipc::move_window_to_workspace(self.active_window_address, target);
        if self.config.behavior.switch_on_move {
            ipc::switch_workspace(target);
        }
        true
    }

    /// Ask the compositor to close `address` and drop it from the local model so
    /// the overlay updates without waiting for the next workspace refresh.
    fn close_window(&mut self, address: u64) {
        ipc::close_window(address);
        self.forget_window(address);
    }

    /// Apply a close locally. Kept separate from the IPC call so the state
    /// transition can be tested without a compositor.
    fn forget_window(&mut self, address: u64) {
        for ws in &mut self.workspaces {
            ws.clients.retain(|c| c.address != address);
        }
        if self.hovered_window == Some(address) {
            self.hovered_window = None;
        }
        if self.tab_target == Some(address) {
            self.tab_target = None;
        }
        self.update_filter_state();
        self.clamp_selected();
        self.needs_redraw = true;
    }

    fn workspace_at(&self, x: f64, y: f64) -> Option<usize> {
        let visible = self.visible_workspace_indices();
        let n = visible.len();
        if n == 0 || self.width == 0 || self.height == 0 {
            return None;
        }

        let cols = self.grid_cols(n);
        let rows = n.div_ceil(cols);
        let pad = self.config.appearance.card_padding;

        let card_w = ((self.width as f64 - pad * (cols + 1) as f64) / cols as f64)
            .min(self.config.appearance.max_card_width);
        let card_h = ((self.height as f64 - pad * (rows + 1) as f64) / rows as f64)
            .min(self.config.appearance.max_card_height);

        let grid_w = cols as f64 * card_w + (cols.saturating_sub(1)) as f64 * pad;
        let grid_h = rows as f64 * card_h + (rows.saturating_sub(1)) as f64 * pad;
        let ox = (self.width as f64 - grid_w) / 2.0;
        let oy = (self.height as f64 - grid_h) / 2.0;

        for i in 0..n {
            let col = i % cols;
            let row = i / cols;
            let cx = ox + col as f64 * (card_w + pad);
            let cy = oy + row as f64 * (card_h + pad);
            if x >= cx && x <= cx + card_w && y >= cy && y <= cy + card_h {
                return Some(i);
            }
        }
        None
    }

    fn select_workspace_at(&mut self, x: f64, y: f64) {
        if let Some(i) = self.workspace_at(x, y) {
            if self.selected != i {
                self.selected = i;
                self.needs_redraw = true;
            }
        }
    }

    // Returns the address of the window under (x, y), or None if the click
    // landed on the card background or outside all cards.
    fn window_at(&self, x: f64, y: f64) -> Option<u64> {
        let visible = self.visible_workspace_indices();
        let n = visible.len();
        if n == 0 || self.width == 0 || self.height == 0 {
            return None;
        }
        let cols = self.grid_cols(n);
        let rows = n.div_ceil(cols);
        let pad = self.config.appearance.card_padding;
        let card_w = ((self.width as f64 - pad * (cols + 1) as f64) / cols as f64)
            .min(self.config.appearance.max_card_width);
        let card_h = ((self.height as f64 - pad * (rows + 1) as f64) / rows as f64)
            .min(self.config.appearance.max_card_height);
        let grid_w = cols as f64 * card_w + (cols.saturating_sub(1)) as f64 * pad;
        let grid_h = rows as f64 * card_h + (rows.saturating_sub(1)) as f64 * pad;
        let ox = (self.width as f64 - grid_w) / 2.0;
        let oy = (self.height as f64 - grid_h) / 2.0;
        let lh = self.config.appearance.label_height;
        let tp = self.config.appearance.thumb_padding;

        for (i, &wi) in visible.iter().enumerate() {
            let ws = &self.workspaces[wi];
            let col = i % cols;
            let row = i / cols;
            let cx = ox + col as f64 * (card_w + pad);
            let cy = oy + row as f64 * (card_h + pad);
            if x < cx || x > cx + card_w || y < cy || y > cy + card_h {
                continue;
            }
            // Empty cards have no windows to hit-test.
            if ws.clients.is_empty() {
                return None;
            }
            // Inside this card — hit-test individual windows with the same
            // rectangles the renderer draws.
            let win_area = layout::Rect {
                x: cx + tp,
                y: cy + lh,
                w: card_w - 2.0 * tp,
                h: card_h - lh - tp,
            };
            let rects = layout::window_rects(
                &ws.clients,
                win_area,
                self.config.appearance.window_gap,
                self.config.appearance.wrap_windows,
            );
            for (client, rect) in ws.clients.iter().zip(rects) {
                if rect.contains(x, y) {
                    return Some(client.address);
                }
            }
            return None;
        }
        None
    }
}

// ── wayland Dispatch impls ───────────────────────────────────────────────────

impl Dispatch<wl_registry::WlRegistry, ()> for AppState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "wl_compositor" => {
                state.compositor = Some(registry.bind(name, version.min(4), qh, ()));
            }
            "wl_shm" => {
                state.shm = Some(registry.bind(name, 1, qh, ()));
            }
            "wl_seat" => {
                state.seat = Some(registry.bind(name, version.min(7), qh, ()));
            }
            "zwlr_layer_shell_v1" => {
                state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()));
            }
            "hyprland_toplevel_export_manager_v1" => {
                state.toplevel_export = Some(registry.bind(name, 1, qh, ()));
            }
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(AppState: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(AppState: ignore wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(AppState: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);
wayland_client::delegate_noop!(AppState: ignore HyprlandToplevelExportManagerV1);

impl Dispatch<wl_shm::WlShm, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for AppState {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(caps),
        } = event
        {
            if caps.contains(wl_seat::Capability::Keyboard) {
                seat.get_keyboard(qh, ());
            }
            if state.allow_mouse
                && caps.contains(wl_seat::Capability::Pointer)
                && state.pointer.is_none()
            {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for AppState {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key {
            key,
            state: WEnum::Value(key_state),
            ..
        } = event
        {
            if key == KEYCODE_LEFTSHIFT || key == KEYCODE_RIGHTSHIFT {
                state.shift_held = key_state == wl_keyboard::KeyState::Pressed;
                return;
            }
            if key_state == wl_keyboard::KeyState::Pressed {
                let keysym = keycode_to_keysym(key);
                if keysym != 0 && state.handle_key(keysym) {
                    state.should_close = true;
                }
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for AppState {
    fn event(
        state: &mut Self,
        pointer: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if !state.allow_mouse {
            return;
        }
        match event {
            wl_pointer::Event::Enter {
                serial,
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer_x = surface_x;
                state.pointer_y = surface_y;
                state.cursor_enter_serial = serial;
                state.cursor_is_pointer = false;
                if state.tab_target.is_some() {
                    state.tab_target = None;
                    state.needs_redraw = true;
                }
                state.select_workspace_at(surface_x, surface_y);
                state.init_cursor(conn, qh);
                let win = state.window_at(surface_x, surface_y);
                state.apply_cursor(pointer, win.is_some());
                state.cursor_is_pointer = win.is_some();
                if state.hovered_window != win {
                    state.hovered_window = win;
                    state.needs_redraw = true;
                }
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer_x = surface_x;
                state.pointer_y = surface_y;
                if state.tab_target.is_some() {
                    state.tab_target = None;
                    state.needs_redraw = true;
                }
                state.select_workspace_at(surface_x, surface_y);
                let win = state.window_at(surface_x, surface_y);
                if win.is_some() != state.cursor_is_pointer {
                    state.apply_cursor(pointer, win.is_some());
                    state.cursor_is_pointer = win.is_some();
                }
                if state.hovered_window != win {
                    state.hovered_window = win;
                    state.needs_redraw = true;
                }
            }
            wl_pointer::Event::Button {
                button,
                state: WEnum::Value(wl_pointer::ButtonState::Pressed),
                ..
            } => {
                if state
                    .workspace_at(state.pointer_x, state.pointer_y)
                    .is_none()
                {
                    state.should_close = true;
                    return;
                }
                match button {
                    BTN_LEFT if state.shift_held => {
                        if state.move_active_window_to_selected() {
                            state.should_close = true;
                        }
                    }
                    BTN_LEFT => {
                        let win = state.window_at(state.pointer_x, state.pointer_y);
                        if let Some(addr) = win {
                            let client = state
                                .workspaces
                                .iter()
                                .flat_map(|ws| ws.clients.iter())
                                .find(|c| c.address == addr);
                            if let Some(c) = client {
                                state.pending_focus =
                                    Some((c.workspace_id, addr, c.x + c.w / 2, c.y + c.h / 2));
                            }
                        } else {
                            state.activate_selected_workspace();
                        }
                        state.should_close = true;
                    }
                    BTN_RIGHT => {
                        // Closing is a multi-step action, so keep the overlay
                        // open after each close instead of dismissing it.
                        if let Some(addr) = state.window_at(state.pointer_x, state.pointer_y) {
                            state.close_window(addr);
                        }
                    }
                    _ => {}
                }
            }
            wl_pointer::Event::Leave { serial, .. } => {
                pointer.set_cursor(serial, None, 0, 0);
                state.cursor_is_pointer = false;
                if state.hovered_window.is_some() {
                    state.hovered_window = None;
                    state.needs_redraw = true;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for AppState {
    fn event(
        state: &mut Self,
        layer_surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                layer_surface.ack_configure(serial);
                state.width = width;
                state.height = height;
                state.configured = true;
                // A Configure can also arrive later, for example when the
                // output is resized while the overlay is visible. The first
                // one happens before `visible` flips on and is drawn by the
                // show path, so only ask for a repaint once we're on screen.
                if state.visible {
                    state.needs_redraw = true;
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                state.should_close = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<HyprlandToplevelExportFrameV1, ()> for AppState {
    fn event(
        state: &mut Self,
        _: &HyprlandToplevelExportFrameV1,
        event: hyprland_toplevel_export_frame_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use hyprland_toplevel_export_frame_v1::Event;
        let Some(cap) = state.pending_capture.as_mut() else {
            return;
        };
        match event {
            Event::Buffer {
                format,
                width,
                height,
                stride,
            } => {
                // Convert WEnum<wl_shm::Format> to u32
                cap.format = format.into();
                cap.width = width;
                cap.height = height;
                cap.stride = stride;
            }
            Event::BufferDone => cap.buffer_done = true,
            Event::Ready { .. } => cap.frame_ready = true,
            Event::Failed => cap.frame_failed = true,
            _ => {}
        }
    }
}

// ── Wayland surface management ───────────────────────────────────────────────

fn create_shm_fd(size: usize) -> Option<OwnedFd> {
    let name = c"hyprview";
    let raw = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
    if raw < 0 {
        return None;
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    if unsafe { libc::ftruncate(raw, size as libc::off_t) } < 0 {
        return None;
    }
    Some(fd)
}

/// Write the whole buffer to `fd`, retrying short writes and EINTR. A single
/// `write()` can legally return fewer bytes than requested (notably for large
/// shm buffers), which would otherwise leave the frame partially filled.
fn write_all_fd(fd: std::os::fd::RawFd, data: &[u8]) -> bool {
    let mut offset = 0;
    while offset < data.len() {
        let n = unsafe {
            libc::write(
                fd,
                data[offset..].as_ptr() as *const libc::c_void,
                data.len() - offset,
            )
        };
        if n < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return false;
        }
        if n == 0 {
            return false;
        }
        offset += n as usize;
    }
    true
}

fn show(state: &mut AppState, qh: &QueueHandle<AppState>) -> bool {
    let (compositor, layer_shell) = match (&state.compositor, &state.layer_shell) {
        (Some(c), Some(l)) => (c, l),
        _ => return false,
    };

    state.should_close = false;
    state.configured = false;

    let wl_surf = compositor.create_surface(qh, ());
    let layer_surface = layer_shell.get_layer_surface(
        &wl_surf,
        None,
        zwlr_layer_shell_v1::Layer::Top,
        "hyprview".to_owned(),
        qh,
        (),
    );

    use zwlr_layer_surface_v1::Anchor;
    layer_surface.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
    layer_surface.set_exclusive_zone(-1);
    layer_surface
        .set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive);

    wl_surf.commit();

    state.wl_surf = Some(wl_surf);
    state.layer_surface = Some(layer_surface);
    true
}

fn cancel_capture(state: &mut AppState) {
    state.capture_queue.clear();
    state.pending_capture = None;
    if let Some(cap) = state.active_capture.take() {
        if let CapturePhase::Phase2 {
            wl_buf,
            mmap_ptr,
            size,
            ..
        } = cap.phase
        {
            unsafe { libc::munmap(mmap_ptr, size) };
            wl_buf.destroy();
        }
        cap.frame.destroy();
    }
}

fn hide(state: &mut AppState) {
    // Cancel any in-flight thumbnail capture
    cancel_capture(state);

    if let Some(buf) = state.wl_buf.take() {
        buf.destroy();
    }
    if let Some(ls) = state.layer_surface.take() {
        ls.destroy();
    }
    if let Some(surf) = state.wl_surf.take() {
        surf.destroy();
    }
    state.configured = false;
    state.visible = false;
    state.filter.clear();
    state.filter_match = None;
    state.tab_target = None;
    state.workspaces.clear();
    state.thumbnails.clear();
    // Reset input state that only makes sense while the overlay is up, so the
    // next show can't render a stale tooltip or start a reversed Tab cycle.
    state.shift_held = false;
    state.hovered_window = None;
    state.pointer_x = 0.0;
    state.pointer_y = 0.0;
    state.cursor_is_pointer = false;
}

fn redraw(state: &mut AppState, qh: &QueueHandle<AppState>) {
    if !state.configured || state.width == 0 || state.height == 0 {
        return;
    }
    state.clamp_selected();

    // `selected`, hit-testing, and keyboard navigation all index the visible
    // list. Build the same list here so the highlight can never land on a
    // different card than the one the user selected. The unfiltered list can
    // be borrowed directly when empty workspaces are shown.
    let visible_workspaces: Vec<WorkspaceInfo>;
    let workspaces_to_draw: &[WorkspaceInfo] = if state.config.behavior.show_empty_workspaces {
        &state.workspaces
    } else {
        visible_workspaces = state
            .visible_workspace_indices()
            .into_iter()
            .map(|i| state.workspaces[i].clone())
            .collect();
        &visible_workspaces
    };

    let pixels = render::draw(render::DrawParams {
        width: state.width,
        height: state.height,
        workspaces: workspaces_to_draw,
        selected_index: state.selected,
        thumbnails: &state.thumbnails,
        cfg: &state.config,
        active_window_address: state.active_window_address,
        hovered_window: state.hovered_window,
        pointer_x: state.pointer_x,
        pointer_y: state.pointer_y,
        filter_match: state.filter_match,
        tab_target: state.tab_target,
        filter: &state.filter,
        icon_cache: &mut state.icon_cache,
    });

    let stride = state.width * 4;
    let size = (stride * state.height) as usize;

    let Some(fd) = create_shm_fd(size) else {
        return;
    };
    let raw_fd = fd.as_fd().as_raw_fd();

    if !write_all_fd(raw_fd, &pixels) {
        return;
    }

    let Some(shm) = &state.shm else { return };
    let pool = shm.create_pool(fd.as_fd(), size as i32, qh, ());
    drop(fd); // compositor mapped it; safe to close our end

    let buf = pool.create_buffer(
        0,
        state.width as i32,
        state.height as i32,
        stride as i32,
        wl_shm::Format::Argb8888,
        qh,
        (),
    );
    pool.destroy();

    if let Some(surf) = &state.wl_surf {
        surf.attach(Some(&buf), 0, 0);
        surf.damage_buffer(0, 0, state.width as i32, state.height as i32);
        surf.commit();
    }

    if let Some(old) = state.wl_buf.replace(buf) {
        old.destroy();
    }
}

// ── async thumbnail capture ──────────────────────────────────────────────────

// Advance the capture state machine by one step. Call once per main-loop
// iteration while the overlay is visible.
fn advance_capture(conn: &Connection, state: &mut AppState, qh: &QueueHandle<AppState>) {
    const TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

    let active = match state.active_capture.take() {
        Some(a) => a,
        None => {
            // Start the next capture if the queue is non-empty. Drop the queue
            // entirely when previews can't be captured, otherwise it would keep
            // the main loop polling at 50 ms intervals for nothing.
            if state.no_preview || state.toplevel_export.is_none() {
                state.capture_queue.clear();
                return;
            }
            let address = match state.capture_queue.pop_front() {
                Some(a) => a,
                None => return,
            };
            let frame =
                state
                    .toplevel_export
                    .as_ref()
                    .unwrap()
                    .capture_toplevel(0, address as u32, qh, ());
            state.pending_capture = Some(CaptureFrameData::default());
            conn.flush().ok();
            state.active_capture = Some(ActiveCapture {
                frame,
                address,
                phase: CapturePhase::Phase1,
                started: std::time::Instant::now(),
            });
            return;
        }
    };

    let timed_out = active.started.elapsed() > TIMEOUT;

    match active.phase {
        CapturePhase::Phase1 => {
            let cap = match state.pending_capture.as_ref() {
                Some(c) => c,
                None => {
                    active.frame.destroy();
                    return;
                }
            };

            if timed_out || cap.frame_failed {
                active.frame.destroy();
                state.pending_capture = None;
                return;
            }

            if !cap.buffer_done {
                state.active_capture = Some(active);
                return;
            }

            // buffer_done — transition to phase 2
            let cap = state.pending_capture.take().unwrap();
            if cap.width == 0 || cap.height == 0 {
                active.frame.destroy();
                return;
            }

            let size = (cap.stride * cap.height) as usize;
            let raw_fd = unsafe { libc::memfd_create(c"hyprview-cap".as_ptr(), libc::MFD_CLOEXEC) };
            if raw_fd < 0 {
                active.frame.destroy();
                return;
            }
            unsafe { libc::ftruncate(raw_fd, size as libc::off_t) };

            let mmap_ptr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    raw_fd,
                    0,
                )
            };
            if mmap_ptr == libc::MAP_FAILED {
                unsafe { libc::close(raw_fd) };
                active.frame.destroy();
                return;
            }

            let owned_fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
            let Some(shm) = &state.shm else {
                unsafe { libc::munmap(mmap_ptr, size) };
                active.frame.destroy();
                return;
            };

            let wl_fmt = wl_shm::Format::try_from(cap.format).unwrap_or(wl_shm::Format::Argb8888);
            let pool = shm.create_pool(owned_fd.as_fd(), size as i32, qh, ());
            let wl_buf = pool.create_buffer(
                0,
                cap.width as i32,
                cap.height as i32,
                cap.stride as i32,
                wl_fmt,
                qh,
                (),
            );
            pool.destroy();
            drop(owned_fd);

            state.pending_capture = Some(CaptureFrameData {
                width: cap.width,
                height: cap.height,
                stride: cap.stride,
                format: cap.format,
                ..Default::default()
            });
            active.frame.copy(&wl_buf, 1);
            conn.flush().ok();

            state.active_capture = Some(ActiveCapture {
                frame: active.frame,
                address: active.address,
                phase: CapturePhase::Phase2 {
                    wl_buf,
                    mmap_ptr,
                    size,
                    width: cap.width,
                    height: cap.height,
                    stride: cap.stride,
                },
                started: active.started,
            });
        }

        CapturePhase::Phase2 {
            wl_buf,
            mmap_ptr,
            size,
            width,
            height,
            stride,
        } => {
            let cap = state.pending_capture.as_ref();
            let ready = cap.is_some_and(|c| c.frame_ready);
            let failed = timed_out || cap.is_none_or(|c| c.frame_failed);

            if !ready && !failed {
                state.active_capture = Some(ActiveCapture {
                    frame: active.frame,
                    address: active.address,
                    phase: CapturePhase::Phase2 {
                        wl_buf,
                        mmap_ptr,
                        size,
                        width,
                        height,
                        stride,
                    },
                    started: active.started,
                });
                return;
            }

            state.pending_capture = None;

            if ready {
                let pixels =
                    unsafe { std::slice::from_raw_parts(mmap_ptr as *const u8, size).to_vec() };
                if let Ok(surface) = cairo::ImageSurface::create_for_data(
                    pixels,
                    cairo::Format::ARgb32,
                    width as i32,
                    height as i32,
                    stride as i32,
                ) {
                    state.thumbnails.push(Thumbnail {
                        address: active.address,
                        surface,
                    });
                }
                state.needs_redraw = true;
            }

            unsafe { libc::munmap(mmap_ptr, size) };
            wl_buf.destroy();
            active.frame.destroy();
        }
    }
}

// ── main ─────────────────────────────────────────────────────────────────────

fn lock_file_path() -> String {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    format!("{runtime_dir}/hyprview.lock")
}

/// Open the lock file without following symlinks and without inheriting a
/// permissive mode. The `/tmp` fallback (used when `XDG_RUNTIME_DIR` is unset)
/// is world-writable, so a symlink planted there must not be followed.
fn open_lock_file_at(path: &std::path::Path) -> Option<std::fs::File> {
    use std::os::unix::ffi::OsStrExt;
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let fd = unsafe {
        libc::open(
            cpath.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o600 as libc::c_uint,
        )
    };
    if fd < 0 {
        return None;
    }
    let file = unsafe { std::fs::File::from_raw_fd(fd) };
    // Reject directories, FIFOs, devices, etc. (O_NOFOLLOW already rejects
    // symlinks). Only a regular file may hold the instance lock.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 || (st.st_mode & libc::S_IFMT) != libc::S_IFREG {
        return None;
    }
    // Tighten an existing file that may have been created with a wider mode.
    unsafe { libc::fchmod(fd, 0o600) };
    Some(file)
}

fn open_lock_file() -> Option<std::fs::File> {
    open_lock_file_at(std::path::Path::new(&lock_file_path()))
}

fn try_lock_instance() -> Option<std::fs::File> {
    use std::io::Write;
    let file = open_lock_file()?;
    let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if ret != 0 {
        return None;
    }
    // Now that we hold the lock, replace any stale PID with ours so --toggle can find us.
    file.set_len(0).ok();
    (&file)
        .write_all(std::process::id().to_string().as_bytes())
        .ok();
    Some(file)
}

fn send_toggle() {
    let path = lock_file_path();
    let Some(file) = open_lock_file() else { return };
    let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if ret == 0 {
        // Got the lock — no daemon running, release and exit
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
        return;
    }
    drop(file);
    if let Ok(s) = std::fs::read_to_string(&path) {
        if let Ok(pid) = s.trim().parse::<i32>() {
            unsafe { libc::kill(pid, libc::SIGUSR1) };
        }
    }
}

fn main() {
    let opts = cli::options();
    if opts.toggle {
        send_toggle();
        return;
    }

    let _lock = match try_lock_instance() {
        Some(f) => f,
        None => std::process::exit(0),
    };

    // Install signal handlers and block them outside ppoll to eliminate the race
    // where a signal fires between the G_TOGGLE check and the poll call.
    let unblocked_mask = unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = sig_handler as *const () as libc::sighandler_t;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGUSR1, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGHUP, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut());

        // Block these signals normally; ppoll atomically unblocks them during
        // the wait so they can never land in the gap before poll starts.
        let mut block_mask: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut block_mask);
        libc::sigaddset(&mut block_mask, libc::SIGUSR1);
        libc::sigaddset(&mut block_mask, libc::SIGHUP);
        libc::sigaddset(&mut block_mask, libc::SIGINT);
        libc::sigaddset(&mut block_mask, libc::SIGTERM);
        libc::sigprocmask(libc::SIG_BLOCK, &block_mask, std::ptr::null_mut());

        let mut empty: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut empty);
        empty
    };

    let conn = Connection::connect_to_env().expect("failed to connect to Wayland display");
    let mut eq: EventQueue<AppState> = conn.new_event_queue();
    let qh = eq.handle();

    let config = Config::load();
    let mut state = AppState::new(opts.no_preview, opts.allow_mouse, config);

    // Register registry listener and perform two roundtrips to discover all globals
    conn.display().get_registry(&qh, ());
    eq.roundtrip(&mut state).expect("Wayland roundtrip failed");
    eq.roundtrip(&mut state).expect("Wayland roundtrip failed");

    if state.compositor.is_none() || state.shm.is_none() || state.layer_shell.is_none() {
        cli::error(format_args!("missing required Wayland globals"));
        std::process::exit(1);
    }

    if state.toplevel_export.is_none() && !state.no_preview {
        cli::warn(format_args!(
            "hyprland-toplevel-export protocol unavailable; \
             window previews disabled (colored rectangles will be used)"
        ));
    }

    cli::info(format_args!(
        "daemon running on {} (SIGUSR1 to toggle, SIGHUP to reload config)",
        ipc::compositor().name()
    ));

    let display_fd = conn.as_fd().as_raw_fd();

    // Watch the config file's parent directory for writes/renames so we can
    // auto-reload when the file changes. Editors often do atomic saves by
    // writing a temp file then renaming it, so IN_MOVED_TO is essential.
    let config_path = config::config_path();
    let config_filename = config_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config.toml")
        .to_owned();
    let inotify_fd = unsafe {
        let fd = libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC);
        if fd >= 0 {
            if let Some(dir) = config_path.parent() {
                if let Ok(cpath) = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes()) {
                    libc::inotify_add_watch(
                        fd,
                        cpath.as_ptr(),
                        libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO,
                    );
                }
            }
        }
        fd
    };

    // Users who bind hyprview directly as their overview key (rather than
    // running it as an autostart daemon and toggling it) can ask for the
    // first overlay to open without a separate `--toggle` call.
    if state.config.behavior.show_on_startup {
        G_TOGGLE.store(true, Ordering::Relaxed);
    }

    'main: loop {
        if G_QUIT.load(Ordering::Relaxed) {
            break;
        }

        if G_RELOAD.swap(false, Ordering::Relaxed) {
            let new_config = Config::load();
            let selected_name = state.selected_workspace().map(|ws| ws.name.clone());
            state.keys = ResolvedKeys::from_config(&new_config.keybindings);
            state.config = new_config;
            // Re-merge CLI flags so behavior changes take effect without a
            // restart.
            let was_no_preview = state.no_preview;
            state.no_preview = state.cli_no_preview || state.config.behavior.no_preview;
            if state.no_preview && !was_no_preview {
                cancel_capture(&mut state);
                state.thumbnails.clear();
                if state.visible {
                    state.needs_redraw = true;
                }
            } else if was_no_preview
                && !state.no_preview
                && state.visible
                && state.toplevel_export.is_some()
            {
                // Preview capture was disabled while the overlay was open;
                // start filling in thumbnails now that it's enabled again.
                state.capture_queue.extend(
                    state
                        .workspaces
                        .iter()
                        .flat_map(|ws| ws.clients.iter().map(|c| c.address)),
                );
            }
            let was_allow_mouse = state.allow_mouse;
            state.allow_mouse = state.cli_allow_mouse || state.config.behavior.allow_mouse;
            if state.allow_mouse && !was_allow_mouse {
                if let Some(seat) = state.seat.as_ref() {
                    if state.pointer.is_none() {
                        state.pointer = Some(seat.get_pointer(&qh, ()));
                    }
                }
            }
            state.icon_cache.refresh();
            if state.visible {
                state.select_workspace_by_name(selected_name.as_deref());
                state.needs_redraw = true;
            }
            cli::info(format_args!("config reloaded"));
        }

        if G_TOGGLE.swap(false, Ordering::Relaxed) {
            if state.visible {
                hide(&mut state);
                conn.flush().ok();
            } else if show(&mut state, &qh) {
                conn.flush().ok();
                eq.roundtrip(&mut state).ok();
                if state.configured {
                    // Fetch workspace/window data (fast IPC calls)
                    state.active_window_address = ipc::get_active_window_address();
                    state.workspaces = ipc::get_workspaces();
                    state.thumbnails.clear();
                    let active = ipc::get_active_workspace_name();
                    state.selected = 0;
                    state.select_workspace_by_name(Some(&active));
                    state.visible = true;

                    // Enqueue thumbnail captures; overlay draws immediately
                    // with colored rectangles and redraws as thumbnails arrive.
                    // Skip the queue entirely when previews are unavailable so
                    // the loop doesn't poll needlessly.
                    if !state.no_preview && state.toplevel_export.is_some() {
                        state.capture_queue.extend(
                            state
                                .workspaces
                                .iter()
                                .flat_map(|ws| ws.clients.iter().map(|c| c.address)),
                        );
                    }

                    redraw(&mut state, &qh);
                    conn.flush().ok();
                }
            }
        }

        if state.should_close && state.visible {
            hide(&mut state);
            conn.flush().ok();
            state.should_close = false;
        }

        if let Some((ws_id, addr, cx, cy)) = state.pending_focus.take() {
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(100));
                ipc::focus_window(ws_id, addr, cx, cy);
            });
        }

        if state.needs_redraw && state.visible {
            state.needs_redraw = false;
            redraw(&mut state, &qh);
            conn.flush().ok();
        }

        // Advance the async thumbnail capture state machine.
        if state.visible {
            advance_capture(&conn, &mut state, &qh);
        }

        conn.flush().ok();

        // Poll for events using prepare_read. Use a short timeout while
        // captures are pending so we can check for per-capture timeouts.
        let capturing = state.active_capture.is_some() || !state.capture_queue.is_empty();
        let guard = match eq.prepare_read() {
            Some(g) => g,
            None => {
                eq.dispatch_pending(&mut state).ok();
                // If there's nothing happening, yield so we don't busy-spin
                // when the compositor keeps sending bookkeeping events (buffer
                // releases, delete_ids, etc.) faster than ppoll can throttle us.
                if !state.visible && !capturing {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                continue;
            }
        };

        let poll_timeout = if capturing {
            &libc::timespec {
                tv_sec: 0,
                tv_nsec: 50_000_000,
            } as *const libc::timespec
        } else {
            std::ptr::null()
        };

        let mut pollfds = [
            libc::pollfd {
                fd: display_fd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: inotify_fd,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let nfds = if inotify_fd >= 0 { 2u64 } else { 1u64 };

        let ret = unsafe { libc::ppoll(pollfds.as_mut_ptr(), nfds, poll_timeout, &unblocked_mask) };

        if ret < 0 {
            drop(guard);
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                // signal interrupted poll — loop back to check flags
                eq.dispatch_pending(&mut state).ok();
                continue;
            }
            break 'main;
        }

        // Check inotify before Wayland so a config change doesn't race a redraw
        if inotify_fd >= 0 && pollfds[1].revents & libc::POLLIN != 0 {
            let mut buf = [0u8; 4096];
            let n =
                unsafe { libc::read(inotify_fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            if n > 0 {
                let ev_size = std::mem::size_of::<libc::inotify_event>();
                let mut offset = 0usize;
                while offset + ev_size <= n as usize {
                    let ev = unsafe { &*(buf.as_ptr().add(offset) as *const libc::inotify_event) };
                    let name_len = ev.len as usize;
                    if name_len > 0 {
                        let name_bytes = &buf[offset + ev_size..offset + ev_size + name_len];
                        let nul = name_bytes.iter().position(|&b| b == 0).unwrap_or(name_len);
                        if let Ok(name) = std::str::from_utf8(&name_bytes[..nul]) {
                            if name == config_filename {
                                G_RELOAD.store(true, Ordering::Relaxed);
                            }
                        }
                    }
                    offset += ev_size + name_len;
                }
            }
        }

        if pollfds[0].revents & (libc::POLLHUP | libc::POLLERR) != 0 {
            drop(guard);
            break 'main;
        }

        if pollfds[0].revents & libc::POLLIN != 0 {
            guard.read().ok();
        } else {
            drop(guard);
        }

        eq.dispatch_pending(&mut state).ok();
    }

    hide(&mut state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{ClientInfo, WorkspaceInfo};

    fn client(address: u64, class: &str, title: &str) -> ClientInfo {
        ClientInfo {
            address,
            class_name: class.into(),
            title: title.into(),
            initial_title: String::new(),
            workspace_id: 1,
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        }
    }

    fn workspace(name: &str, clients: Vec<ClientInfo>) -> WorkspaceInfo {
        WorkspaceInfo {
            id: 1,
            name: name.into(),
            monitor_id: 0,
            clients,
        }
    }

    // ── workspace_matches ────────────────────────────────────────────────────

    #[test]
    fn workspace_matches_by_name() {
        let ws = workspace("web", vec![]);
        assert!(workspace_matches(&ws, "web"));
        assert!(workspace_matches(&ws, "we"));
        assert!(!workspace_matches(&ws, "code"));
    }

    #[test]
    fn workspace_matches_by_client_class() {
        let ws = workspace("1", vec![client(1, "firefox", "New Tab")]);
        assert!(workspace_matches(&ws, "fire"));
        assert!(!workspace_matches(&ws, "zed"));
    }

    #[test]
    fn workspace_matches_by_client_title() {
        let ws = workspace("1", vec![client(1, "foot", "nvim — main.rs")]);
        assert!(workspace_matches(&ws, "nvim"));
        assert!(!workspace_matches(&ws, "emacs"));
    }

    #[test]
    fn workspace_matches_empty_filter_always_true() {
        let ws = workspace("1", vec![]);
        assert!(workspace_matches(&ws, ""));
    }

    // ── find_single_window_match ─────────────────────────────────────────────

    #[test]
    fn single_match_returns_address() {
        let workspaces = vec![workspace(
            "1",
            vec![client(10, "firefox", "GitHub"), client(20, "foot", "nvim")],
        )];
        assert_eq!(find_single_window_match(&workspaces, "fire"), Some(10));
    }

    #[test]
    fn multiple_matches_returns_none() {
        let workspaces = vec![workspace(
            "1",
            vec![
                client(10, "firefox", "GitHub"),
                client(20, "foot", "Firefox bookmarks"),
            ],
        )];
        assert_eq!(find_single_window_match(&workspaces, "firefox"), None);
    }

    #[test]
    fn no_matches_returns_none() {
        let workspaces = vec![workspace("1", vec![client(10, "foot", "nvim")])];
        assert_eq!(find_single_window_match(&workspaces, "zed"), None);
    }

    #[test]
    fn single_match_across_workspaces() {
        let workspaces = vec![
            workspace("1", vec![client(10, "foot", "nvim")]),
            workspace("2", vec![client(20, "firefox", "GitHub")]),
        ];
        assert_eq!(find_single_window_match(&workspaces, "github"), Some(20));
    }

    #[test]
    fn match_is_case_insensitive() {
        let workspaces = vec![workspace("1", vec![client(10, "Alacritty", "bash")])];
        assert_eq!(find_single_window_match(&workspaces, "ALACRITTY"), Some(10));
    }

    // ── tab_candidates ───────────────────────────────────────────────────────

    #[test]
    fn tab_candidates_empty_filter_returns_all_in_grid_order() {
        let workspaces = vec![
            workspace(
                "1",
                vec![client(10, "foot", "term"), client(11, "firefox", "web")],
            ),
            workspace("2", vec![client(20, "zed", "editor")]),
        ];
        assert_eq!(tab_candidates(&workspaces, ""), vec![10, 11, 20]);
    }

    #[test]
    fn tab_candidates_sorts_by_position_not_list_order() {
        // ws.clients arrives in Hyprland's internal list order; tab_candidates
        // must reorder by (y, x) so cycling follows the visual grid, not
        // whatever order the compositor happened to return.
        let mut rightmost = client(11, "discord", "chat");
        rightmost.x = 1917;
        let mut leftmost = client(10, "firefox", "web");
        leftmost.x = -1102;
        let workspaces = vec![workspace("1", vec![rightmost, leftmost])];
        assert_eq!(tab_candidates(&workspaces, ""), vec![10, 11]);
    }

    #[test]
    fn tab_candidates_filter_narrows_to_matches() {
        let workspaces = vec![
            workspace(
                "1",
                vec![client(10, "foot", "term"), client(11, "firefox", "web")],
            ),
            workspace("2", vec![client(20, "firefox", "docs")]),
        ];
        assert_eq!(tab_candidates(&workspaces, "firefox"), vec![11, 20]);
    }

    #[test]
    fn tab_candidates_no_matches_returns_empty() {
        let workspaces = vec![workspace("1", vec![client(10, "foot", "term")])];
        assert_eq!(tab_candidates(&workspaces, "zed"), Vec::<u64>::new());
    }

    // ── next_tab_target ──────────────────────────────────────────────────────

    #[test]
    fn next_tab_target_starts_after_focused_window() {
        // Focus is on the first window; the first Tab should move to the next
        // one, not restart at the first window.
        assert_eq!(next_tab_target(&[10, 11, 20], 11, false), Some(20));
    }

    #[test]
    fn next_tab_target_wraps_forward_from_last() {
        assert_eq!(next_tab_target(&[10, 11, 20], 20, false), Some(10));
    }

    #[test]
    fn next_tab_target_reverse_starts_before_focused_window() {
        assert_eq!(next_tab_target(&[10, 11, 20], 11, true), Some(10));
    }

    #[test]
    fn next_tab_target_reverse_wraps_from_first() {
        assert_eq!(next_tab_target(&[10, 11, 20], 10, true), Some(20));
    }

    #[test]
    fn next_tab_target_falls_back_to_first_when_focus_not_candidate() {
        // Focus outside the candidate set (e.g. filtered out): forward starts
        // at the top, reverse at the bottom.
        assert_eq!(next_tab_target(&[10, 11], 99, false), Some(10));
        assert_eq!(next_tab_target(&[10, 11], 99, true), Some(11));
    }

    #[test]
    fn next_tab_target_empty_candidates_returns_none() {
        assert_eq!(next_tab_target(&[], 10, false), None);
        assert_eq!(next_tab_target(&[], 10, true), None);
    }

    // ── key mapping functions ────────────────────────────────────────────────

    #[test]
    fn keycode_15_maps_to_tab() {
        assert_eq!(keycode_to_keysym(15), XKB_KEY_TAB);
    }

    #[test]
    fn key_name_tab_resolves() {
        assert_eq!(key_name_to_keysym("Tab"), Some(XKB_KEY_TAB));
    }

    // ── visible workspace mapping ────────────────────────────────────────────

    fn test_state(show_empty: bool) -> AppState {
        let mut cfg = Config::default();
        cfg.behavior.show_empty_workspaces = show_empty;
        AppState::new(false, false, cfg)
    }

    fn three_workspaces() -> Vec<WorkspaceInfo> {
        vec![
            workspace("1", vec![]),
            workspace("2", vec![client(10, "foot", "term")]),
            workspace("3", vec![]),
        ]
    }

    #[test]
    fn visible_indices_hide_empty_workspaces_by_default() {
        let mut st = test_state(false);
        st.workspaces = three_workspaces();
        assert_eq!(st.visible_workspace_indices(), vec![1]);
    }

    #[test]
    fn visible_indices_include_empty_workspaces_when_configured() {
        let mut st = test_state(true);
        st.workspaces = three_workspaces();
        assert_eq!(st.visible_workspace_indices(), vec![0, 1, 2]);
    }

    #[test]
    fn selected_index_maps_into_the_visible_list() {
        let mut st = test_state(false);
        st.workspaces = three_workspaces();
        st.selected = 0;
        assert_eq!(
            st.selected_workspace().map(|ws| ws.name.as_str()),
            Some("2")
        );
    }

    #[test]
    fn clamp_selected_survives_a_shrinking_visible_list() {
        let mut st = test_state(false);
        st.workspaces = three_workspaces();
        st.selected = 2;
        st.clamp_selected();
        assert_eq!(st.selected, 0);
    }

    #[test]
    fn selection_follows_the_same_workspace_when_visibility_changes() {
        let mut st = test_state(false);
        st.workspaces = three_workspaces();
        // Only workspace "2" is visible, so it is index 0.
        st.selected = 0;
        let name = st.selected_workspace().unwrap().name.clone();

        st.config.behavior.show_empty_workspaces = true;
        st.select_workspace_by_name(Some(&name));

        assert_eq!(st.selected, 1);
        assert_eq!(
            st.selected_workspace().map(|ws| ws.name.as_str()),
            Some("2")
        );
    }

    #[test]
    fn workspace_at_returns_a_visible_index() {
        let mut st = test_state(false);
        st.workspaces = three_workspaces();
        st.width = 1000;
        st.height = 800;

        // Only workspace "2" is visible, so the center card maps to index 0.
        assert_eq!(st.workspace_at(500.0, 400.0), Some(0));
        assert_eq!(
            st.selected_workspace().map(|ws| ws.name.as_str()),
            Some("2")
        );
    }

    #[test]
    fn window_at_empty_card_returns_none_without_overflowing() {
        let mut st = test_state(true);
        st.workspaces = vec![workspace("1", vec![])];
        st.width = 1000;
        st.height = 800;

        assert_eq!(st.window_at(500.0, 400.0), None);
    }

    #[test]
    fn window_at_hits_the_wrapped_layout_the_renderer_draws() {
        let clients: Vec<ClientInfo> = (0..4)
            .map(|i| {
                let mut c = client(i + 1, "foot", "term");
                c.x = (i % 2) as i32 * 800;
                c.y = (i / 2) as i32 * 600;
                c.w = 800;
                c.h = 600;
                c
            })
            .collect();
        let mut st = test_state(false);
        st.workspaces = vec![workspace("1", clients.clone())];
        st.width = 1000;
        st.height = 800;

        // Mirror the card geometry used by `window_at` and `render::draw`.
        let pad = st.config.appearance.card_padding;
        let cols = st.grid_cols(1);
        let rows = 1;
        let card_w = ((st.width as f64 - pad * (cols + 1) as f64) / cols as f64)
            .min(st.config.appearance.max_card_width);
        let card_h = ((st.height as f64 - pad * (rows + 1) as f64) / rows as f64)
            .min(st.config.appearance.max_card_height);
        let area = layout::Rect {
            x: (st.width as f64 - card_w) / 2.0 + st.config.appearance.thumb_padding,
            y: (st.height as f64 - card_h) / 2.0 + st.config.appearance.label_height,
            w: card_w - 2.0 * st.config.appearance.thumb_padding,
            h: card_h - st.config.appearance.label_height - st.config.appearance.thumb_padding,
        };
        let rects = layout::window_rects(
            &clients,
            area,
            st.config.appearance.window_gap,
            st.config.appearance.wrap_windows,
        );

        // The third window wraps onto the second row; clicking its center must
        // select it rather than falling through to the card background.
        let target = rects[2];
        assert_eq!(
            st.window_at(target.x + target.w / 2.0, target.y + target.h / 2.0),
            Some(clients[2].address)
        );
    }

    // ── keyboard state transitions ───────────────────────────────────────────

    #[test]
    fn arrow_key_clears_tab_target_even_when_selection_cannot_move() {
        let mut st = test_state(false);
        st.workspaces = vec![workspace("1", vec![client(10, "foot", "term")])];
        st.tab_target = Some(10);

        assert!(!st.handle_key(XKB_KEY_LEFT));
        assert_eq!(st.tab_target, None);
    }

    #[test]
    fn clearing_a_filter_keeps_the_current_selection() {
        let mut st = test_state(false);
        st.workspaces = vec![
            workspace("1", vec![client(10, "foot", "term")]),
            workspace("2", vec![client(20, "firefox", "web")]),
        ];
        st.selected = 1;

        st.filter.clear();
        st.update_filter_state();

        assert_eq!(st.selected, 1);
    }

    #[test]
    fn non_empty_filter_selects_the_first_matching_visible_workspace() {
        let mut st = test_state(false);
        st.workspaces = vec![
            workspace("1", vec![client(10, "foot", "term")]),
            workspace("2", vec![client(20, "firefox", "web")]),
        ];
        st.filter = "fire".into();

        st.update_filter_state();

        assert_eq!(st.selected, 1);
    }

    #[test]
    fn forgetting_a_closed_window_clears_its_transient_selection() {
        let mut st = test_state(false);
        st.workspaces = vec![workspace(
            "1",
            vec![client(10, "foot", "term"), client(20, "firefox", "web")],
        )];
        st.hovered_window = Some(10);
        st.tab_target = Some(10);

        st.forget_window(10);

        assert!(st.workspaces[0].clients.iter().all(|c| c.address != 10));
        assert_eq!(st.hovered_window, None);
        assert_eq!(st.tab_target, None);
        assert!(st.needs_redraw);
    }

    #[test]
    fn filter_change_clears_a_tab_target_it_excludes() {
        let mut st = test_state(false);
        st.workspaces = vec![workspace(
            "1",
            vec![client(10, "foot", "term"), client(20, "firefox", "web")],
        )];
        st.tab_target = Some(10);
        st.filter = "fire".into();

        st.update_filter_state();

        assert_eq!(st.tab_target, None);
    }

    #[test]
    fn confirm_ignores_a_stale_tab_target_under_a_filter() {
        let mut st = test_state(false);
        st.workspaces = vec![workspace(
            "1",
            vec![client(10, "foot", "term"), client(20, "firefox", "web")],
        )];
        st.filter = "foot".into();
        st.filter_match = Some(10);
        // Bypass update_filter_state so the stale target survives; Confirm
        // must still reject it and use the filter match.
        st.tab_target = Some(20);

        assert!(st.handle_key(XKB_KEY_RETURN));
        assert_eq!(st.pending_focus.map(|(_, addr, _, _)| addr), Some(10));
    }

    #[test]
    fn out_of_range_number_key_is_a_no_op() {
        let mut st = test_state(false);
        st.workspaces = vec![workspace("1", vec![client(10, "foot", "term")])];

        assert!(!st.handle_key(XKB_KEY_1 + 4));
    }

    #[test]
    fn hide_resets_per_show_input_state() {
        let mut st = test_state(false);
        st.visible = true;
        st.shift_held = true;
        st.hovered_window = Some(10);
        st.filter_match = Some(10);
        st.pointer_x = 12.0;
        st.pointer_y = 34.0;
        st.cursor_is_pointer = true;

        hide(&mut st);

        assert!(!st.visible);
        assert!(!st.shift_held);
        assert_eq!(st.hovered_window, None);
        assert_eq!(st.filter_match, None);
        assert_eq!(st.pointer_x, 0.0);
        assert_eq!(st.pointer_y, 0.0);
        assert!(!st.cursor_is_pointer);
    }

    // ── lock file hardening ──────────────────────────────────────────────────

    #[test]
    fn open_lock_file_refuses_symlinks_and_non_regular_files() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let dir = std::env::temp_dir().join(format!("hyprview-lock-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let target = dir.join("target");
        std::fs::write(&target, "x").unwrap();
        let link = dir.join("link");
        symlink(&target, &link).unwrap();
        assert!(open_lock_file_at(&link).is_none());
        assert!(open_lock_file_at(&dir).is_none());

        let file = open_lock_file_at(&dir.join("lock")).expect("regular file should open");
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        drop(file);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_all_fd_writes_the_whole_buffer() {
        let fd = unsafe { libc::memfd_create(c"hyprview-write-test".as_ptr(), libc::MFD_CLOEXEC) };
        assert!(fd >= 0);
        let data = vec![7u8; 1 << 20];

        assert!(write_all_fd(fd, &data));
        unsafe { libc::lseek(fd, 0, libc::SEEK_SET) };

        let mut out = vec![0u8; data.len()];
        let mut offset = 0usize;
        while offset < out.len() {
            let n = unsafe {
                libc::read(
                    fd,
                    out[offset..].as_mut_ptr() as *mut libc::c_void,
                    out.len() - offset,
                )
            };
            assert!(n > 0);
            offset += n as usize;
        }
        assert_eq!(out, data);
        unsafe { libc::close(fd) };
    }
}

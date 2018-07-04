//! Awesome compatibility modules

extern crate cairo;
extern crate cairo_sys;
extern crate env_logger;
extern crate getopts;
extern crate gdk_pixbuf;
extern crate glib;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate nix;
extern crate rlua;
extern crate xcb;
extern crate wayland_client;

extern crate byteorder; extern crate tempfile;
use byteorder::{NativeEndian, WriteBytesExt};


// TODO remove
extern crate wlroots;
use wlroots::{KeyboardModifier, key_events::KeyEvent, wlr_key_state::*};

#[macro_use]
mod macros;

mod objects;
mod common;

mod awesome;
mod keygrabber;
mod mousegrabber;
mod root;
mod lua;

use std::{env, mem, path::PathBuf, process::exit};
use std::cmp::min;

use wayland_client::protocol::wl_compositor::RequestsTrait as CompositorRequests;
use wayland_client::protocol::wl_display::RequestsTrait as DisplayRequests;
use wayland_client::protocol::wl_shell::RequestsTrait as ShellRequests;
use wayland_client::protocol::wl_shell_surface::RequestsTrait as ShellSurfaceRequests;
use wayland_client::protocol::wl_shm::RequestsTrait as ShmRequests;
use wayland_client::protocol::wl_shm_pool::RequestsTrait as PoolRequests;
use wayland_client::protocol::wl_surface::RequestsTrait as SurfaceRequests;
use wayland_client::protocol::{wl_compositor, wl_seat, wl_shell, wl_shell_surface, wl_shm};
use std::io::Write;
use std::os::unix::io::AsRawFd;


use lua::setup_lua;
use rlua::{LightUserData, Lua, Table};
use log::LogLevel;
use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet};
use xcb::{xkb, Connection};
use wayland_client::{Display, EventQueue, GlobalManager, Proxy};
use wayland_client::protocol::wl_display::RequestsTrait;
use wayland_client::sys::client::wl_display;

use self::lua::{LUA, NEXT_LUA};


use self::objects::key::Key;
use self::common::{object::{Object, Objectable}, signal::*};
use self::root::ROOT_KEYS_HANDLE;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");
const GIT_VERSION: &'static str = include_str!(concat!(env!("OUT_DIR"), "/git-version.txt"));
pub const GLOBAL_SIGNALS: &'static str = "__awesome_global_signals";
pub const XCB_CONNECTION_HANDLE: &'static str = "__xcb_connection";

/// Called from `wayland_glib_interface.c` after every call back into the
/// wayland event loop.
///
/// This restarts the Lua thread if there is a new one pending
#[no_mangle]
pub extern "C" fn refresh_awesome() {
    error!("refresh_awesome called");
    NEXT_LUA.with(|new_lua_check| {
        if new_lua_check.get() {
            new_lua_check.set(false);
            LUA.with(|lua| {
                let mut lua = lua.borrow_mut();
                unsafe {
                    *lua = rlua::Lua::new_with_debug();
                }
            });
            setup_lua();
        }
    });
}

fn main() {
    let mut opts = getopts::Options::new();
    opts.optflag("", "version", "show version information");
    let matches = match opts.parse(env::args().skip(1)) {
        Ok(m) => m,
        Err(f) => {
            eprintln!("{}", f.to_string());
            exit(1);
        }
    };
    if matches.opt_present("version") {
        if !GIT_VERSION.is_empty() {
            println!("Way Cooler {} @ {}", VERSION, GIT_VERSION);
        } else {
            println!("Way Cooler {}", VERSION);
        }
        return
    }
    init_logs();
    let sig_action = SigAction::new(SigHandler::Handler(sig_handle),
                                    SaFlags::empty(),
                                    SigSet::empty());
    unsafe {
        signal::sigaction(signal::SIGINT, &sig_action).expect("Could not set SIGINT catcher");
    }
    let (display, mut event_queue) = init_wayland();
    let globals = GlobalManager::new(display.get_registry().unwrap());

    // roundtrip to retrieve the globals list
    event_queue.sync_roundtrip().unwrap();

    /*
     * Create a buffer with window contents
     */

    // buffer (and window) width and height
    let buf_x: u32 = 320;
    let buf_y: u32 = 240;

    // create a tempfile to write the conents of the window on
    let mut tmp = tempfile::tempfile().ok().expect("Unable to create a tempfile.");
    // write the contents to it, lets put a nice color gradient
    for i in 0..(buf_x * buf_y) {
        let x = (i % buf_x) as u32;
        let y = (i / buf_x) as u32;
        let r: u32 = min(((buf_x - x) * 0xFF) / buf_x, ((buf_y - y) * 0xFF) / buf_y);
        let g: u32 = min((x * 0xFF) / buf_x, ((buf_y - y) * 0xFF) / buf_y);
        let b: u32 = min(((buf_x - x) * 0xFF) / buf_x, (y * 0xFF) / buf_y);
        let _ = tmp.write_u32::<NativeEndian>((0xFF << 24) + (r << 16) + (g << 8) + b);
    }
    let _ = tmp.flush();

    /*
     * Init wayland objects
     */

    // The compositor allows us to creates surfaces
    let compositor = globals
        .instantiate_auto::<wl_compositor::WlCompositor>()
        .unwrap()
        .implement(|_, _| {});
    let surface = compositor.create_surface().unwrap().implement(|_, _| {});

    // The SHM allows us to share memory with the server, and create buffers
    // on this shared memory to paint our surfaces
    let shm = globals
        .instantiate_auto::<wl_shm::WlShm>()
        .unwrap()
        .implement(|_, _| {});
    let pool = shm.create_pool(
        tmp.as_raw_fd(),            // RawFd to the tempfile serving as shared memory
        (buf_x * buf_y * 4) as i32, // size in bytes of the shared memory (4 bytes per pixel)
    ).unwrap()
        .implement(|_, _| {});
    let buffer = pool.create_buffer(
        0,                        // Start of the buffer in the pool
        buf_x as i32,             // width of the buffer in pixels
        buf_y as i32,             // height of the buffer in pixels
        (buf_x * 4) as i32,       // number of bytes between the beginning of two consecutive lines
        wl_shm::Format::Argb8888, // chosen encoding for the data
    ).unwrap()
        .implement(|_, _| {});

    // The shell allows us to define our surface as a "toplevel", meaning the
    // server will treat it as a window
    //
    // NOTE: the wl_shell interface is actually deprecated in favour of the xdg_shell
    // protocol, available in wayland-protocols. But this will do for this example.
    let shell = globals
        .instantiate_auto::<wl_shell::WlShell>()
        .unwrap()
        .implement(|_, _| {});
    let shell_surface = shell.get_shell_surface(&surface).unwrap().implement(
        |event, shell_surface: Proxy<wl_shell_surface::WlShellSurface>| {
            use wayland_client::protocol::wl_shell_surface::{Event, RequestsTrait};
            // This ping/pong mechanism is used by the wayland server to detect
            // unresponsive applications
            if let Event::Ping { serial } = event {
                shell_surface.pong(serial);
            }
        },
    );

    // Set our surface as toplevel and define its contents
    shell_surface.set_toplevel();
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();

    // initialize a seat to retrieve pointer events
    // to be handled properly this should be more dynamic, as more
    // than one seat can exist (and they can be created and destroyed
    // dynamically), however most "traditional" setups have a single
    // seat, so we'll keep it simple here
    let mut pointer_created = false;
    let _seat = globals.instantiate_auto::<wl_seat::WlSeat>().unwrap().implement(
        move |event, seat: Proxy<wl_seat::WlSeat>| {
            // The capabilities of a seat are known at runtime and we retrieve
            // them via an events. 3 capabilities exists: pointer, keyboard, and touch
            // we are only interested in pointer here
            use wayland_client::protocol::wl_pointer::Event as PointerEvent;
            use wayland_client::protocol::wl_seat::{Capability, Event as SeatEvent,
                                                    RequestsTrait as SeatRequests};

            if let SeatEvent::Capabilities { capabilities } = event {
                if !pointer_created && capabilities.contains(Capability::Pointer) {
                    // create the pointer only once
                    pointer_created = true;
                    seat.get_pointer().unwrap().implement(|event, _| match event {
                        PointerEvent::Enter {
                            surface_x, surface_y, ..
                        } => {
                            println!("Pointer entered at ({}, {}).", surface_x, surface_y);
                        }
                        PointerEvent::Leave { .. } => {
                            println!("Pointer left.");
                        }
                        PointerEvent::Motion {
                            surface_x, surface_y, ..
                        } => {
                            println!("Pointer moved to ({}, {}).", surface_x, surface_y);
                        }
                        PointerEvent::Button { button, state, .. } => {
                            println!("Button {} was {:?}.", button, state);
                        }
                        _ => {}
                    });
                }
            }
        },
    );
    lua::setup_lua();
    lua::enter_glib_loop();
}

fn init_wayland() -> (Display, EventQueue) {
    let (display, mut event_queue) = match Display::connect_to_env() {
        Ok(res) => res,
        Err(err) => {
            error!("Could not connect to Wayland server. Is it running?");
            exit(1);
        }
    };
    unsafe {
        #[link(name = "wayland_glib_interface", kind = "static")]
        extern "C" {
            fn wayland_glib_interface_init(display: *mut wl_display);
        }
        wayland_glib_interface_init(display.c_ptr() as *mut wl_display);
    }
    (display, event_queue)
}

fn setup_awesome_path(lua: &Lua) -> rlua::Result<()> {
    let globals = lua.globals();
    let package: Table = globals.get("package")?;
    let mut path = package.get::<_, String>("path")?;
    let mut cpath = package.get::<_, String>("cpath")?;

    for mut xdg_data_path in
        env::var("XDG_DATA_DIRS").unwrap_or("/usr/local/share:/usr/share".into())
                                 .split(':')
                                 .map(PathBuf::from)
    {
        xdg_data_path.push("awesome/lib");
        path.push_str(&format!(";{0}/?.lua;{0}/?/init.lua",
                               xdg_data_path.as_os_str().to_string_lossy()));
        cpath.push_str(&format!(";{}/?.so", xdg_data_path.into_os_string().to_string_lossy()));
    }

    for mut xdg_config_path in env::var("XDG_CONFIG_DIRS").unwrap_or("/etc/xdg".into())
                                                          .split(':')
                                                          .map(PathBuf::from)
    {
        xdg_config_path.push("awesome");
        cpath.push_str(&format!(";{}/?.so",
                                xdg_config_path.into_os_string().to_string_lossy()));
    }

    package.set("path", path)?;
    package.set("cpath", cpath)?;

    Ok(())
}

/// Set up global signals value
///
/// We need to store this in Lua, because this make it safer to use.
fn setup_global_signals(lua: &Lua) -> rlua::Result<()> {
    lua.set_named_registry_value(GLOBAL_SIGNALS, lua.create_table()?)
}

/// Sets up the xcb connection and stores it in Lua (for us to access it later)
fn setup_xcb_connection(lua: &Lua) -> rlua::Result<()> {
    let con = match Connection::connect(None) {
        Err(err) => {
            error!("Way Cooler requires XWayland in order to function");
            error!("However, xcb could not connect to it. Is it running?");
            error!("{:?}", err);
            panic!("Could not connect to XWayland instance");
        }
        Ok(con) => con.0
    };
    // Tell xcb we are using the xkb extension
    match xkb::use_extension(&con, 1, 0).get_reply() {
        Ok(r) => {
            if !r.supported() {
                panic!("xkb-1.0 is not supported");
            }
        }
        Err(err) => {
            panic!("Could not get xkb extension supported version {:?}", err);
        }
    }
    lua.set_named_registry_value(XCB_CONNECTION_HANDLE,
                                  LightUserData(con.get_raw_conn() as _))?;
    mem::forget(con);
    Ok(())
}

/// Emits the Awesome keybindinsg.
fn emit_awesome_keybindings(lua: &Lua,
                            event: &KeyEvent,
                            event_modifiers: KeyboardModifier)
                            -> rlua::Result<()> {
    let state_string = if event.key_state() == WLR_KEY_PRESSED {
        "press"
    } else {
        "release"
    };
    // TODO Should also emit by current focused client so we can
    // do client based rules.
    let keybindings = lua.named_registry_value::<Vec<rlua::AnyUserData>>(ROOT_KEYS_HANDLE)?;
    for event_keysym in event.pressed_keys() {
        for binding in &keybindings {
            let obj: Object = binding.clone().into();
            let key = Key::cast(obj.clone()).unwrap();
            let keycode = key.keycode()?;
            let keysym = key.keysym()?;
            let modifiers = key.modifiers()?;
            let binding_match = (keysym != 0 && keysym == event_keysym
                                 || keycode != 0 && keycode == event.keycode())
                                && modifiers == 0
                                || modifiers == event_modifiers.bits();
            if binding_match {
                emit_object_signal(&*lua, obj, state_string.into(), event_keysym)?;
            }
        }
    }
    Ok(())
}

/// Formats the log strings properly
fn log_format(record: &log::LogRecord) -> String {
    let color = match record.level() {
        LogLevel::Info => "",
        LogLevel::Trace => "\x1B[37m",
        LogLevel::Debug => "\x1B[44m",
        LogLevel::Warn => "\x1B[33m",
        LogLevel::Error => "\x1B[31m"
    };
    let location = record.location();
    let file = location.file();
    let line = location.line();
    let mut module_path = location.module_path();
    if let Some(index) = module_path.find("way_cooler::") {
        let index = index + "way_cooler::".len();
        module_path = &module_path[index..];
    }
    format!("{} {} [{}] \x1B[37m{}:{}\x1B[0m{0} {} \x1B[0m",
            color,
            record.level(),
            module_path,
            file,
            line,
            record.args())
}

fn init_logs() {
    let mut builder = env_logger::LogBuilder::new();
    builder.format(log_format);
    builder.filter(None, log::LogLevelFilter::Trace);
    if env::var("WAY_COOLER_LOG").is_ok() {
        builder.parse(&env::var("WAY_COOLER_LOG").expect("WAY_COOLER_LOG not defined"));
    }
    builder.init().expect("Unable to initialize logging!");
    info!("Logger initialized");
}

/// Handler for SIGINT signal
extern "C" fn sig_handle(_: nix::libc::c_int) {
    lua::terminate();
}

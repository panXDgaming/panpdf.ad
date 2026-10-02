#![forbid(unsafe_code)]
#![cfg_attr(target_os = "android", allow(dead_code))]

pub mod app;

#[cfg(not(target_arch = "wasm32"))]
mod ai_actions;
#[cfg(not(target_arch = "wasm32"))]
mod ai_panel;
#[cfg(not(target_arch = "wasm32"))]
mod ai_written;
#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "android")]
pub mod android_tools;
mod canvas;
mod chooser;
mod chrome;
mod clipboard;
mod contents;
mod dialog;
mod draw_pen;
mod draw_shape;
mod drawing_speed;
mod drop_zone;
mod field_properties;
mod fields_panel;
mod fill_form;
mod find_bar;
mod form_tool;
mod format;
mod hub;
pub use hub::pdfs_in;
mod icons;
mod input;
mod interface_fonts;
mod link_tool;
mod live_typing;
#[cfg(not(target_arch = "wasm32"))]
mod memory;
mod menus;
mod meter;
mod moment;
mod naming;
mod ocr_tool;
mod order;
mod own_folder;
mod page_actions;
mod page_motion;
mod pages;
mod palette;
mod pictures;
mod place_picture;
mod print_tool;
mod properties;
#[cfg(not(target_arch = "wasm32"))]
mod reporting;
mod room;
pub mod save_file;
mod shortcuts;
mod side_panel;
mod stamp_tool;
#[cfg(not(target_arch = "wasm32"))]
pub mod startup;
mod status_line;
mod system_dialog;
mod take_out;
mod task;
mod text;
#[cfg(not(target_arch = "wasm32"))]
mod tools_marks;
#[cfg(not(target_arch = "wasm32"))]
mod tools_page;
#[cfg(not(target_arch = "wasm32"))]
mod tools_room;
#[cfg(not(target_arch = "wasm32"))]
mod tools_run;
#[cfg(not(target_arch = "wasm32"))]
mod tools_screens;
#[cfg(not(target_arch = "wasm32"))]
mod tools_settings;
mod touch;
mod trace;
mod unlock;
mod view_mode;
#[cfg(target_arch = "wasm32")]
pub mod web_files;
#[cfg(target_arch = "wasm32")]
mod web_ocr;
#[cfg(target_arch = "wasm32")]
pub mod web_tiles;
mod window_state;

pub use window_state::ZOOMS;

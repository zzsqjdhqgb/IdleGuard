//! Windows-specific services. All Win32 calls are contained in this module.

pub mod autostart;
pub mod icon;
pub mod idle;
pub mod shutdown;
pub mod single_instance;
pub mod tray;
pub mod window;

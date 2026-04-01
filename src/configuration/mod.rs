//! Configuration and theme loading.
//!
//! This module owns config parsing/validation and theme conversion used at app startup.

pub mod reader;
pub mod theme;

pub use reader::Configuration;
pub use reader::read_configuration;
pub use reader::save_scale_to_config;

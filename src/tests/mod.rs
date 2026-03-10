// This file declares the `json` submodule within the `tests` module.
#[cfg(test)]
pub mod json;

// Notebook operation tests (create/delete/move/load metadata).
#[cfg(test)]
pub mod notebook;

// Configuration reader tests.
#[cfg(test)]
pub mod configuration;

// Component and state tests.
#[cfg(test)]
pub mod components;

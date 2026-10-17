pub mod ast;
pub mod checking;
pub mod compiler;
pub mod diagnostic;
pub mod lexer;
pub mod lowering;
pub mod parser;
pub mod resolution;
pub mod source;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod event_tests;

pub mod bundle;
mod bundle_link;
pub(crate) mod bundle_prepare;
pub mod bundle_registration;
#[cfg(test)]
mod composite_tests;
#[cfg(test)]
mod composition_tests;
#[cfg(test)]
mod container_tests;
#[cfg(test)]
mod image_tests;
#[cfg(test)]
mod reload_tests;

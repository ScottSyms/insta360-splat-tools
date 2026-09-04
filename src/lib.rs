// Shared library for both binaries.
// insta-keyframes binary keeps its own `mod` declarations in src/main.rs for now.
// This lib exposes the mask pipeline modules for insta-mask and future shared code.

pub mod mask;
pub mod people;
pub mod objects;
pub mod shadows;
pub mod temporal;
pub mod image;
pub mod insta_mask_manifest;

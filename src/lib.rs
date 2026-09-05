// Shared library for both binaries.
// imu-keyframes binary keeps its own `mod` declarations in src/main.rs for now.
// This lib exposes the mask pipeline modules for scene-mask and future shared code.

pub mod mask;
pub mod people;
pub mod objects;
pub mod shadows;
pub mod temporal;
pub mod image;
pub mod insta_mask_manifest;

// Spec3: geometry-assisted COLMAP pipeline
pub mod capture;
pub mod geometry;
pub mod pairs;
pub mod colmap;
pub mod metadata;
pub mod keyframes;

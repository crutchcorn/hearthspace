//! Grouped pinned windows may reveal into the canvas without covering neighboring controls.
use super::HearthspaceRenderElement;
use smithay::{
    backend::renderer::element::utils::CropRenderElement,
    utils::{Logical, Rectangle},
};

pub(super) fn clip_elements(
    elements: &mut Vec<HearthspaceRenderElement>,
    front: usize,
    clip: Rectangle<i32, Logical>,
) {
    let clip = clip.to_physical(1);
    let clipped: Vec<_> = elements
        .drain(front..)
        .filter_map(|element| match element {
            HearthspaceRenderElement::Surface(inner) => {
                CropRenderElement::from_element(inner, 1., clip)
                    .map(HearthspaceRenderElement::ClippedSurface)
            }
            HearthspaceRenderElement::Frame(inner) => {
                CropRenderElement::from_element(inner, 1., clip)
                    .map(HearthspaceRenderElement::CroppedFrame)
            }
            HearthspaceRenderElement::Memory(inner) => {
                CropRenderElement::from_element(inner, 1., clip)
                    .map(HearthspaceRenderElement::CroppedMemory)
            }
            HearthspaceRenderElement::RoundedMemory(inner) => {
                CropRenderElement::from_element(inner, 1., clip)
                    .map(HearthspaceRenderElement::CroppedRoundedMemory)
            }
            HearthspaceRenderElement::RoundedSurface(inner) => {
                CropRenderElement::from_element(inner, 1., clip)
                    .map(HearthspaceRenderElement::CroppedRoundedSurface)
            }
            HearthspaceRenderElement::Texture(inner) => {
                CropRenderElement::from_element(inner, 1., clip)
                    .map(HearthspaceRenderElement::CroppedTexture)
            }
            other => Some(other),
        })
        .collect();
    elements.extend(clipped);
}

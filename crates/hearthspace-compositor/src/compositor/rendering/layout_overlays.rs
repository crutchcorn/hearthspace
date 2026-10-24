//! Object-owned outlines follow their surface; frame guides stay below the tools.
use std::collections::HashMap;

use hearthspace_common::frame_layout::TargetKind;
use smithay::{
    backend::renderer::gles::GlesRenderer,
    utils::{Logical, Rectangle},
};

use super::{App, HearthspaceRenderElement};

pub(super) struct LayoutOverlays {
    pub front: Vec<HearthspaceRenderElement>,
    pub back: Vec<HearthspaceRenderElement>,
    pub modal: Vec<HearthspaceRenderElement>,
    pub objects: HashMap<u64, Vec<HearthspaceRenderElement>>,
    pub owners: Vec<(usize, u64)>,
}

impl App {
    pub(super) fn layout_overlay_elements(
        &mut self,
        renderer: &mut GlesRenderer,
        output: Rectangle<i32, Logical>,
    ) -> LayoutOverlays {
        let (back, mut shapes) = self.layout_overlay_shapes();
        let front_count = shapes.len();
        shapes.extend(back);
        let modal_index = shapes.len();
        shapes.extend(self.modal_confirmation_shape(output));
        let elements = self.layout_editor.overlays.elements(renderer, &shapes);
        let mut result = LayoutOverlays {
            front: Vec::new(),
            back: Vec::new(),
            modal: Vec::new(),
            objects: HashMap::new(),
            owners: Vec::new(),
        };
        // The cache skips invisible shapes, so retain the corresponding shape
        // identity before assigning an element to its layer or native owner.
        for ((index, shape), element) in shapes
            .iter()
            .enumerate()
            .filter(|(_, shape)| {
                shape.opacity > 0. && shape.bounds.width > 0. && shape.bounds.height > 0.
            })
            .zip(elements)
        {
            if index == modal_index {
                result.modal.push(element);
            } else if index >= front_count {
                result.back.push(element);
            } else {
                let owner = self.layout_outline_owner(&shape.key);
                if let Some((id, TargetKind::Item | TargetKind::Widget | TargetKind::Group)) = owner
                {
                    // Selection rings belong to the object, including during a
                    // turn. Independent guides/drop highlights have no owner.
                    result.objects.entry(id).or_default().push(element);
                } else {
                    if let Some((id, _)) = owner {
                        result.owners.push((result.front.len(), id));
                    }
                    result.front.push(element);
                }
            }
        }
        result
    }
}

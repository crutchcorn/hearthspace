//! Texture composition plus original client storage for Smithay plane selection.
use smithay::{
    backend::renderer::{
        element::{
            Element, Id, Kind, RenderElement, UnderlyingStorage, texture::TextureRenderElement,
        },
        gles::{GlesError, GlesFrame, GlesRenderer, GlesTexture},
        utils::{Buffer, CommitCounter, DamageSet, OpaqueRegions},
    },
    utils::{
        Buffer as BufferCoords, Physical, Rectangle, Scale, Transform, user_data::UserDataMap,
    },
};

pub(super) struct SceneElement {
    pub texture: TextureRenderElement<GlesTexture>,
    pub client: Option<Buffer>,
}

impl Element for SceneElement {
    fn id(&self) -> &Id {
        self.texture.id()
    }
    fn current_commit(&self) -> CommitCounter {
        self.texture.current_commit()
    }
    fn src(&self) -> Rectangle<f64, BufferCoords> {
        self.texture.src()
    }
    fn transform(&self) -> Transform {
        self.texture.transform()
    }
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.texture.geometry(scale)
    }
    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.texture.damage_since(scale, commit)
    }
    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        self.texture.opaque_regions(scale)
    }
    fn kind(&self) -> Kind {
        if self.client.is_some() {
            Kind::ScanoutCandidate
        } else {
            Kind::Unspecified
        }
    }
}

impl RenderElement<GlesRenderer> for SceneElement {
    fn underlying_storage(&self, _renderer: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        self.client.as_ref().map(UnderlyingStorage::Wayland)
    }
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, BufferCoords>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        <TextureRenderElement<GlesTexture> as RenderElement<GlesRenderer>>::draw(
            &self.texture,
            frame,
            src,
            dst,
            damage,
            opaque_regions,
            cache,
        )
    }
}

use std::any::Any;
use std::collections::HashMap;
use std::ffi::c_char;
use std::fmt::Debug;
use std::marker::PhantomData;

use drm_sys::{
    drm_mode_modeinfo, DRM_MODE_OBJECT_BLOB, DRM_MODE_OBJECT_CONNECTOR, DRM_MODE_OBJECT_ENCODER,
    DRM_MODE_OBJECT_PROPERTY, DRM_PROP_NAME_LEN,
};
use syscall::{Error, Result, EINVAL};

use crate::GraphicsAdapter;

#[derive(Debug)]
pub struct DrmObjects<T: GraphicsAdapter> {
    next_id: DrmObjectId,
    connectors: Vec<DrmObjectId>,
    encoders: Vec<DrmObjectId>,
    objects: HashMap<DrmObjectId, DrmObjectData>,
    _marker: PhantomData<T>,
}

impl<T: GraphicsAdapter> DrmObjects<T> {
    pub(crate) fn new() -> Self {
        DrmObjects {
            next_id: DrmObjectId(1),
            connectors: vec![],
            encoders: vec![],
            objects: HashMap::new(),
            _marker: PhantomData,
        }
    }

    fn add<U: DrmObject>(&mut self, data: U) -> DrmObjectId {
        let id = self.next_id;
        self.objects.insert(
            id,
            DrmObjectData {
                kind: Box::new(data),
                properties: vec![],
            },
        );
        self.next_id.0 += 1;

        id
    }

    fn get<U: DrmObject>(&self, id: DrmObjectId) -> Result<&U> {
        let object = self.objects.get(&id).ok_or(Error::new(EINVAL))?;
        if let Some(object) = (&*object.kind as &dyn Any).downcast_ref::<U>() {
            Ok(object)
        } else {
            Err(Error::new(EINVAL))
        }
    }

    fn get_mut<U: DrmObject>(&mut self, id: DrmObjectId) -> Result<&mut U> {
        let object = self.objects.get_mut(&id).ok_or(Error::new(EINVAL))?;
        if let Some(object) = (&mut *object.kind as &mut dyn Any).downcast_mut::<U>() {
            Ok(object)
        } else {
            Err(Error::new(EINVAL))
        }
    }

    pub fn object_type(&self, id: DrmObjectId) -> Result<u32> {
        let object = self.objects.get(&id).ok_or(Error::new(EINVAL))?;
        Ok(object.kind.object_type())
    }

    pub fn add_property(
        &mut self,
        name: &str,
        immutable: bool,
        atomic: bool,
        kind: DrmPropertyKind,
    ) -> DrmObjectId {
        if name.len() > DRM_PROP_NAME_LEN as usize {
            panic!("Property name {name} is too long");
        }

        match &kind {
            DrmPropertyKind::Range(start, end) => assert!(start < end),
            DrmPropertyKind::Enum(variants) => {
                // FIXME check duplicate variant numbers
                for (variant_name, _) in variants {
                    if variant_name.len() > DRM_PROP_NAME_LEN as usize {
                        panic!("Property variant name {variant_name} is too long");
                    }
                }
            }
            DrmPropertyKind::Blob => {}
            DrmPropertyKind::Bitmask(bitmask_flags) => {
                // FIXME check overlapping flag numbers
                for (flag_name, _) in bitmask_flags {
                    if flag_name.len() > DRM_PROP_NAME_LEN as usize {
                        panic!("Property bitflag name {flag_name} is too long");
                    }
                }
            }
            DrmPropertyKind::Object => {}
            DrmPropertyKind::SignedRange(start, end) => assert!(start < end),
        }

        let mut name_bytes = [0; DRM_PROP_NAME_LEN as usize];
        for (to, &from) in name_bytes.iter_mut().zip(name.as_bytes()) {
            *to = from as c_char;
        }

        self.add(DrmProperty {
            name: name_bytes,
            immutable,
            atomic,
            kind,
        })
    }

    pub fn get_property(&self, id: DrmObjectId) -> Result<&DrmProperty> {
        self.get(id)
    }

    pub fn add_object_property(&mut self, object: DrmObjectId, property: DrmObjectId, value: u64) {
        let object = self.objects.get_mut(&object).unwrap();
        // FIXME validate property uniqueness and value
        object.properties.push((property, value));
    }

    pub fn set_object_property(&mut self, object: DrmObjectId, property: DrmObjectId, value: u64) {
        let object = self.objects.get_mut(&object).unwrap();
        // FIXME validate property existence and value
        for (prop, val) in object.properties.iter_mut() {
            if *prop == property {
                *val = value;
            }
        }
    }

    pub fn get_object_properties(&self, id: DrmObjectId) -> Result<&[(DrmObjectId, u64)]> {
        let object = self.objects.get(&id).ok_or(Error::new(EINVAL))?;
        Ok(&object.properties)
    }

    pub fn add_blob(&mut self, data: Vec<u8>) -> DrmObjectId {
        self.add(DrmBlob { data })
    }

    pub fn get_blob(&self, id: DrmObjectId) -> Result<&[u8]> {
        Ok(&self.get::<DrmBlob>(id)?.data)
    }

    pub fn add_connector(&mut self, driver_data: T::Connector) -> DrmObjectId {
        let connector_id = self.add(DrmConnector {
            modes: vec![],
            encoder_id: DrmObjectId::INVALID,
            connector_type: 0,
            connector_type_id: 0,
            connection: DrmConnectorStatus::Unknown,
            mm_width: 0,
            mm_height: 0,
            subpixel: DrmSubpixelOrder::Unknown,
            driver_data,
        });
        self.connectors.push(connector_id);

        let encoder_id = self.add(DrmEncoder {
            crtc_id: DrmObjectId::INVALID,
            possible_crtcs: 0,
            possible_clones: 0,
        });
        self.encoders.push(encoder_id);

        self.get_connector_mut(connector_id).unwrap().encoder_id = encoder_id;

        connector_id
    }

    pub fn connector_ids(&self) -> &[DrmObjectId] {
        &self.connectors
    }

    pub fn connectors(&self) -> impl Iterator<Item = &DrmConnector<T::Connector>> + use<'_, T> {
        self.connectors.iter().map(|&id| {
            (&self.objects[&id].kind as &dyn Any)
                .downcast_ref::<DrmConnector<T::Connector>>()
                .unwrap()
        })
    }

    pub fn get_connector(&self, id: DrmObjectId) -> Result<&DrmConnector<T::Connector>> {
        self.get(id)
    }

    pub fn get_connector_mut(
        &mut self,
        id: DrmObjectId,
    ) -> Result<&mut DrmConnector<T::Connector>> {
        self.get_mut(id)
    }

    pub fn encoder_ids(&self) -> &[DrmObjectId] {
        &self.encoders
    }

    pub fn get_encoder(&self, id: DrmObjectId) -> Result<&DrmEncoder> {
        self.get(id)
    }

    pub fn get_encoder_mut(&mut self, id: DrmObjectId) -> Result<&mut DrmEncoder> {
        self.get_mut(id)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct DrmObjectId(pub(crate) u32);

impl DrmObjectId {
    pub const INVALID: DrmObjectId = DrmObjectId(0);
}

impl From<DrmObjectId> for u64 {
    fn from(value: DrmObjectId) -> Self {
        value.0.into()
    }
}

#[derive(Debug)]
struct DrmObjectData {
    kind: Box<dyn DrmObject + 'static>,
    properties: Vec<(DrmObjectId, u64)>,
}

pub trait DrmObject: Any + Debug {
    fn object_type(&self) -> u32;
}

#[derive(Debug)]
pub struct DrmProperty {
    pub name: [c_char; DRM_PROP_NAME_LEN as usize],
    pub immutable: bool,
    pub atomic: bool,
    pub kind: DrmPropertyKind,
}

#[derive(Debug)]
pub enum DrmPropertyKind {
    Range(u64, u64),
    Enum(Vec<(&'static str, u64)>),
    Blob,
    Bitmask(Vec<(&'static str, u64)>),
    Object,
    SignedRange(i64, i64),
}

impl DrmObject for DrmProperty {
    fn object_type(&self) -> u32 {
        DRM_MODE_OBJECT_PROPERTY
    }
}

#[derive(Debug)]
pub struct DrmBlob {
    data: Vec<u8>,
}

impl DrmObject for DrmBlob {
    fn object_type(&self) -> u32 {
        DRM_MODE_OBJECT_BLOB
    }
}

#[derive(Debug)]
pub struct DrmConnector<T: Debug + 'static> {
    pub modes: Vec<drm_mode_modeinfo>,
    pub encoder_id: DrmObjectId,
    pub connector_type: u32,
    pub connector_type_id: u32,
    pub connection: DrmConnectorStatus,
    pub mm_width: u32,
    pub mm_height: u32,
    pub subpixel: DrmSubpixelOrder,
    pub driver_data: T,
}

#[derive(Debug, Copy, Clone)]
#[repr(u32)]
pub enum DrmConnectorStatus {
    Disconnected = 0,
    Connected = 1,
    Unknown = 2,
}

#[derive(Debug, Copy, Clone)]
#[repr(u32)]
pub enum DrmSubpixelOrder {
    Unknown = 0,
    HorizontalRGB,
    HorizontalBGR,
    VerticalRGB,
    VerticalBGR,
    None,
}

impl<T: Debug + 'static> DrmObject for DrmConnector<T> {
    fn object_type(&self) -> u32 {
        DRM_MODE_OBJECT_CONNECTOR
    }
}

// FIXME can we represent connector and encoder using a single struct?
#[derive(Debug)]
pub struct DrmEncoder {
    pub crtc_id: DrmObjectId,
    pub possible_crtcs: u32,
    pub possible_clones: u32,
}

impl DrmObject for DrmEncoder {
    fn object_type(&self) -> u32 {
        DRM_MODE_OBJECT_ENCODER
    }
}

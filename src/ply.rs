use anyhow::{bail, Context, Result};
use byteorder::{BigEndian, ByteOrder, LittleEndian};
use glam::{vec3, Vec3};

use crate::{mesh::InputMesh, stream::AsyncStreamReader};

enum Format {
    Ascii,
    BigEndian,
    LittleEndian,
}

enum ScalarType {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

enum DynamicScalar {
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    F32(f32),
    F64(f64),
}

impl DynamicScalar {
    fn as_usize(&self) -> Option<usize> {
        match self {
            Self::I8(v) => Some(*v as usize),
            Self::U8(v) => Some(*v as usize),
            Self::I16(v) => Some(*v as usize),
            Self::U16(v) => Some(*v as usize),
            Self::I32(v) => Some(*v as usize),
            Self::U32(v) => Some(*v as usize),
            _ => None,
        }
    }

    fn as_f32(&self) -> Option<f32> {
        match self {
            Self::F32(v) => Some(*v as f32),
            Self::F64(v) => Some(*v as f32),
            _ => None,
        }
    }
}

impl ScalarType {
    async fn read<O: ByteOrder>(&self, reader: &mut AsyncStreamReader) -> Result<DynamicScalar> {
        match self {
            Self::I8 => Ok(DynamicScalar::I8(reader.read_exact(1).await?[0] as i8)),
            Self::U8 => Ok(DynamicScalar::U8(reader.read_exact(1).await?[0])),
            Self::I16 => Ok(DynamicScalar::I16(O::read_i16(reader.read_exact(2).await?))),
            Self::U16 => Ok(DynamicScalar::U16(O::read_u16(reader.read_exact(2).await?))),
            Self::I32 => Ok(DynamicScalar::I32(O::read_i32(reader.read_exact(4).await?))),
            Self::U32 => Ok(DynamicScalar::U32(O::read_u32(reader.read_exact(4).await?))),
            Self::F32 => Ok(DynamicScalar::F32(O::read_f32(reader.read_exact(4).await?))),
            Self::F64 => Ok(DynamicScalar::F64(O::read_f64(reader.read_exact(8).await?))),
        }
    }
}

enum PropertyType {
    Scalar(ScalarType),
    List(ScalarType, ScalarType),
}

struct Property {
    name: String,
    ty: PropertyType,
}

enum DynamicProperty {
    Scalar(DynamicScalar),
    List(Vec<DynamicScalar>),
}

impl Property {
    async fn read<O: ByteOrder>(&self, reader: &mut AsyncStreamReader) -> Result<DynamicProperty> {
        match &self.ty {
            PropertyType::Scalar(ty) => Ok(DynamicProperty::Scalar(ty.read::<O>(reader).await?)),
            PropertyType::List(len_ty, ty) => {
                let len = len_ty.read::<O>(reader).await?.as_usize().unwrap();
                let mut list = Vec::with_capacity(len);

                for _ in 0..len {
                    list.push(ty.read::<O>(reader).await?);
                }

                Ok(DynamicProperty::List(list))
            }
        }
    }
}

struct Element {
    name: String,
    count: usize,
    properties: Vec<Property>,
}

trait PlyVisitor {
    fn visit_element(self, name: &str) -> Box<dyn ElementVisitor<Self>>;
}

trait ElementVisitor<P: PlyVisitor> {
    fn visit_property(&mut self, name: &str, property: DynamicProperty);
    fn finish(self: Box<Self>) -> P;
}

async fn read_magic(reader: &mut AsyncStreamReader) -> Result<()> {
    let magic = reader.read_line_utf8().await?;
    if magic != "ply" {
        bail!("Invalid PLY magic");
    }
    Ok(())
}

async fn read_format(reader: &mut AsyncStreamReader) -> Result<Format> {
    let mut tokens = reader.read_line_utf8().await?.split_whitespace();

    if tokens.next() != Some("format") {
        bail!("Missing PLY format line");
    }
    match tokens.next() {
        Some("ascii") => Ok(Format::Ascii),
        Some("binary_big_endian") => Ok(Format::BigEndian),
        Some("binary_little_endian") => Ok(Format::LittleEndian),
        _ => bail!("Unknown PLY format"),
    }
}

fn parse_element_line<'a, I: Iterator<Item = &'a str>>(tokens: &mut I) -> Result<(String, usize)> {
    let name = tokens.next().context("Missing element name")?;
    let num = tokens.next().context("Missing element count")?.parse()?;

    Ok((name.to_string(), num))
}

fn parse_scalar_type(token: &str) -> Result<ScalarType> {
    match token {
        "int8" | "char" => Ok(ScalarType::I8),
        "uint8" | "uchar" => Ok(ScalarType::U8),
        "int16" | "short" => Ok(ScalarType::I16),
        "uint16" | "ushort" => Ok(ScalarType::U16),
        "int32" | "int" => Ok(ScalarType::I32),
        "uint32" | "uint" => Ok(ScalarType::U32),
        "float32" | "float" => Ok(ScalarType::F32),
        "float64" | "double" => Ok(ScalarType::F64),
        _ => bail!("Unknown scalar type"),
    }
}

fn parse_property_type<'a, I: Iterator<Item = &'a str>>(tokens: &mut I) -> Result<PropertyType> {
    match tokens.next() {
        Some("list") => Ok(PropertyType::List(
            parse_scalar_type(tokens.next().context("Missing list length type")?)?,
            parse_scalar_type(tokens.next().context("Missing list type")?)?,
        )),
        Some(token) => Ok(PropertyType::Scalar(parse_scalar_type(token)?)),
        _ => bail!("Missing element type"),
    }
}

fn parse_property<'a, I: Iterator<Item = &'a str>>(tokens: &mut I) -> Result<Property> {
    let ty = parse_property_type(tokens)?;
    let name = tokens.next().context("Missing property name")?;

    Ok(Property {
        name: name.to_string(),
        ty,
    })
}

trait Accept<T> {
    fn accept(&mut self, v: T);
}

struct VertexVisitor<V: PlyVisitor + Accept<Vec3>> {
    x: Option<f32>,
    y: Option<f32>,
    z: Option<f32>,
    parent: V,
}

impl<V: PlyVisitor + Accept<Vec3>> VertexVisitor<V> {
    fn new(parent: V) -> Self {
        Self {
            x: None,
            y: None,
            z: None,
            parent,
        }
    }
}

impl<V: PlyVisitor + Accept<Vec3>> ElementVisitor<V> for VertexVisitor<V> {
    fn visit_property(&mut self, name: &str, property: DynamicProperty) {
        match property {
            DynamicProperty::Scalar(s) => match name {
                "x" => self.x = s.as_f32(),
                "y" => self.y = s.as_f32(),
                "z" => self.z = s.as_f32(),
                _ => (),
            },
            DynamicProperty::List(_) => (),
        }
    }

    fn finish(mut self: Box<Self>) -> V {
        self.parent
            .accept(vec3(self.x.unwrap(), self.y.unwrap(), self.z.unwrap()));
        self.parent
    }
}

struct FaceVisitor<V: PlyVisitor + Accept<[usize; 3]>> {
    indices: Option<[usize; 3]>,
    parent: V,
}

impl<V: PlyVisitor + Accept<[usize; 3]>> FaceVisitor<V> {
    fn new(parent: V) -> Self {
        Self {
            indices: None,
            parent,
        }
    }
}

impl<V: PlyVisitor + Accept<[usize; 3]>> ElementVisitor<V> for FaceVisitor<V> {
    fn visit_property(&mut self, name: &str, property: DynamicProperty) {
        match property {
            DynamicProperty::Scalar(_) => (),
            DynamicProperty::List(v) => match name {
                "vertex_indices" => {
                    if v.len() != 3 {
                        unimplemented!();
                    }
                    self.indices = Some([
                        v[0].as_usize().unwrap(),
                        v[1].as_usize().unwrap(),
                        v[2].as_usize().unwrap(),
                    ])
                }
                _ => (),
            },
        }
    }

    fn finish(mut self: Box<Self>) -> V {
        self.parent.accept(self.indices.unwrap());
        self.parent
    }
}

struct AnyElementVisitor<V: PlyVisitor>(V);
impl<V: PlyVisitor> ElementVisitor<V> for AnyElementVisitor<V> {
    fn visit_property(&mut self, _name: &str, _property: DynamicProperty) {}
    fn finish(self: Box<Self>) -> V {
        self.0
    }
}

struct MeshVisitor {
    mesh: InputMesh,
}

impl MeshVisitor {
    fn new() -> Self {
        Self {
            mesh: Default::default(),
        }
    }

    fn finish(mut self) -> InputMesh {
        self.mesh
            .normals
            .resize(self.mesh.vertices.len(), Vec3::ZERO);

        for &[a, b, c] in &self.mesh.tris {
            let v0 = self.mesh.vertices[a];
            let v1 = self.mesh.vertices[b];
            let v2 = self.mesh.vertices[c];

            let n = (v2 - v0).cross(v1 - v0).normalize();

            self.mesh.normals[a] += n;
            self.mesh.normals[b] += n;
            self.mesh.normals[c] += n;
        }

        for n in &mut self.mesh.normals {
            *n = n.normalize();
        }

        self.mesh
    }
}

impl<'a> PlyVisitor for MeshVisitor {
    fn visit_element(self, name: &str) -> Box<dyn ElementVisitor<Self>> {
        match name {
            "vertex" => Box::new(VertexVisitor::new(self)),
            "face" => Box::new(FaceVisitor::new(self)),
            _ => Box::new(AnyElementVisitor(self)),
        }
    }
}

impl Accept<Vec3> for MeshVisitor {
    fn accept(&mut self, v: Vec3) {
        self.mesh.vertices.push(v)
    }
}

impl Accept<[usize; 3]> for MeshVisitor {
    fn accept(&mut self, v: [usize; 3]) {
        self.mesh.tris.push(v)
    }
}

async fn parse_binary<O: ByteOrder>(
    reader: &mut AsyncStreamReader,
    elements: Vec<Element>,
) -> Result<InputMesh> {
    let mut visitor = MeshVisitor::new();
    for element in elements {
        for _ in 0..element.count {
            let mut el_visitor = visitor.visit_element(element.name.as_str());
            for prop in &element.properties {
                let p = prop.read::<O>(reader).await?;
                el_visitor.visit_property(prop.name.as_str(), p);
            }
            visitor = el_visitor.finish();
        }
    }
    Ok(visitor.finish())
}

pub async fn load_ply(reader: &mut AsyncStreamReader) -> Result<InputMesh> {
    read_magic(reader).await?;
    let format = read_format(reader).await?;

    let mut elements = Vec::new();
    let mut parsing_element = None;

    while let Ok(line) = reader.read_line_utf8().await {
        let mut tokens = line.split_whitespace();
        match tokens.next() {
            Some("comment") | None => (),
            Some("element") => {
                if let Some(el) = parsing_element.take() {
                    elements.push(el);
                }

                let (name, count) = parse_element_line(&mut tokens)?;
                parsing_element = Some(Element {
                    name,
                    count,
                    properties: Vec::new(),
                });
            }
            Some("property") => {
                if let Some(el) = parsing_element.as_mut() {
                    el.properties.push(parse_property(&mut tokens)?);
                } else {
                    bail!("Unexpected property line");
                }
            }
            Some("end_header") => {
                if let Some(el) = parsing_element.take() {
                    elements.push(el);
                }
                break;
            }
            _ => log::warn!("Unexpected PLY line"),
        }
    }

    match format {
        Format::Ascii => unimplemented!(),
        Format::BigEndian => parse_binary::<BigEndian>(reader, elements).await,
        Format::LittleEndian => parse_binary::<LittleEndian>(reader, elements).await,
    }
}

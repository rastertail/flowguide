use futures::FutureExt;
use glam::Vec3;
use wasm_bindgen::prelude::*;

use crate::{ply::load_ply, stream::AsyncStreamReader};

#[derive(Clone, Default)]
#[wasm_bindgen]
pub struct InputMesh {
    pub(crate) vertices: Vec<Vec3>,
    pub(crate) normals: Vec<Vec3>,
    pub(crate) tris: Vec<[usize; 3]>,
}

#[wasm_bindgen]
impl InputMesh {
    #[wasm_bindgen(constructor)]
    pub async fn new(file: &web_sys::File) -> Result<InputMesh, JsValue> {
        let js_reader = web_sys::ReadableStreamDefaultReader::new(&file.stream())
            .expect("Could not open file reader");
        let mut reader = AsyncStreamReader::new(move || {
            wasm_bindgen_futures::JsFuture::from(js_reader.read()).map(|r| {
                r.ok()
                    .and_then(|v| js_sys::Reflect::get(&v, &"value".into()).ok())
                    .and_then(|v| v.dyn_into::<js_sys::Uint8Array>().ok())
                    .map(|a| a.to_vec())
            })
        });

        Ok(load_ply(&mut reader).await.map_err(|e| format!("{}", e))?)
    }
}

pub struct ProcessMesh {
    pub vertices: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub tris: Vec<[usize; 3]>,
    pub adjacency_face: Vec<Vec<(usize, usize)>>,
    pub dual_area: Vec<f32>,
}

impl From<InputMesh> for ProcessMesh {
    fn from(input: InputMesh) -> Self {
        let mut adjacency_face = vec![Vec::new(); input.vertices.len()];
        for (i, [a, b, c]) in input.tris.iter().enumerate() {
            adjacency_face[*a].push((*b, i));
            adjacency_face[*b].push((*c, i));
            adjacency_face[*c].push((*a, i));
        }

        let mut dual_area = vec![0f32; input.vertices.len()];
        'outer: for i in 0..input.vertices.len() {
            let src = i;
            let (mut dest, face) = adjacency_face[src][0];
            let mut tri = &input.tris[face];

            let mut circumcenters = Vec::new();

            // im tired
            while {
                dest = tri[(tri.iter().position(|v| *v == dest).unwrap() + 1) % 3];
                dest
            } != adjacency_face[src][0].0
            {
                let a = input.vertices[tri[0]] - input.vertices[tri[2]];
                let b = input.vertices[tri[1]] - input.vertices[tri[2]];
                let axb = a.cross(b);
                circumcenters.push((a.dot(a) * b - b.dot(b) * a).cross(axb) / (2.0 * axb.dot(axb)));

                if let Some((_, next_face)) =
                    adjacency_face[src].iter().filter(|v| v.0 == dest).next()
                {
                    tri = &input.tris[*next_face];
                } else {
                    log::warn!("non manifold vertex {}", i);
                    dual_area[i] = 1.0;
                    continue 'outer;
                }
            }

            let mut v = Vec3::ZERO;
            for i in 0..circumcenters.len() {
                v += circumcenters[i].cross(circumcenters[(i + 1) % circumcenters.len()]);
            }
            dual_area[i] = 0.5 * v.length();
        }

        Self {
            vertices: input.vertices,
            normals: input.normals,
            tris: input.tris,
            adjacency_face,
            dual_area,
        }
    }
}

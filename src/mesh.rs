use glam::Vec3;

#[derive(Default)]
pub struct InputMesh {
    pub vertices: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub tris: Vec<[usize; 3]>,
}

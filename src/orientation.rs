use glam::{vec3, Vec3};
use ordered_float::OrderedFloat;
use rand::{rngs::SmallRng, seq::SliceRandom, Rng, SeedableRng};

use crate::{hierarchy::HierarchyLevel, mesh::ProcessMesh};

fn extrinsic_compat(o0: Vec3, n0: Vec3, o1: Vec3, n1: Vec3) -> (Vec3, Vec3) {
    let p0 = n0.cross(o0);
    let p1 = n1.cross(o1);

    let a = [
        (o0, o1),
        (p0, o1),
        (-o0, o1),
        (-p0, o1),
        (o0, p1),
        (p0, p1),
        (-o0, p1),
        (-p0, p1),
    ];

    a.into_iter()
        .max_by_key(|(a, b)| OrderedFloat(a.dot(*b)))
        .unwrap()
}

fn extrinsic_smooth<R: Rng>(mesh: &ProcessMesh, o_field: &mut [Vec3], rng: &mut R) {
    let mut indices = (0..mesh.vertices.len()).collect::<Vec<_>>();
    indices.shuffle(rng);

    for i in indices {
        let mut o_i = o_field[i];
        let n_i = mesh.normals[i];

        for (weight, (j, _)) in mesh.adjacency_face[i].iter().enumerate() {
            let o_j = o_field[*j];
            let n_j = mesh.normals[*j];

            let (compat_0, compat_1) = extrinsic_compat(o_i, n_i, o_j, n_j);

            o_i = (weight as f32) * compat_0 + compat_1;
            o_i -= n_i * o_i.dot(n_i);
            o_i = o_i.normalize();
        }

        o_field[i] = o_i;
    }
}

pub fn hierarchical_smoothing(hierarchy: &[HierarchyLevel], iterations: usize) -> Vec<Vec3> {
    let mut rng = SmallRng::seed_from_u64(0); // todo do this better

    let mut field = if hierarchy.len() > 1 {
        let coarse_field = hierarchical_smoothing(&hierarchy[0..hierarchy.len() - 1], iterations);
        let mut init = vec![Vec3::ZERO; hierarchy[hierarchy.len() - 1].mesh.vertices.len()];
        for (i, v) in init.iter_mut().enumerate() {
            *v = coarse_field[hierarchy[hierarchy.len() - 1].up_mapping[i]];
        }
        init
    } else {
        let mut init = vec![Vec3::ZERO; hierarchy[0].mesh.vertices.len()];
        for (i, v) in init.iter_mut().enumerate() {
            let n = hierarchy[0].mesh.normals[i];

            let sign = if n.z < 0.0 { -1.0 } else { 1.0 };
            let a = -1.0 / (sign + n.z);
            let b = n.x * n.y * a;
            let x = vec3(1.0 + sign * n.x * n.x * a, sign * b, -sign * n.x);
            let y = vec3(b, sign + n.y * n.y * a, -n.y);
            let theta = rng.gen::<f32>() * std::f32::consts::TAU;

            *v = x * theta.cos() + y * theta.sin();
        }
        init
    };

    for i in 0..iterations {
        extrinsic_smooth(&hierarchy[hierarchy.len() - 1].mesh, &mut field, &mut rng);
    }

    field
}

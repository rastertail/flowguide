use crate::mesh::ProcessMesh;

pub struct HierarchyLevel {
    pub mesh: ProcessMesh,
    pub up_mapping: Vec<usize>,
}

pub fn build(mesh: ProcessMesh) -> Vec<HierarchyLevel> {
    let mut ranking = mesh
        .adjacency_face
        .iter()
        .enumerate()
        .flat_map(|(i, j)| {
            let mesh = &mesh;
            j.iter().map(move |(j, _)| {
                let ai = mesh.dual_area[i];
                let aj = mesh.dual_area[*j];
                let ratio = if ai > aj { ai / aj } else { aj / ai };
                let rank = mesh.normals[i].dot(mesh.normals[*j]) * ratio;
                (i, *j, rank)
            })
        })
        .collect::<Vec<_>>();

    if ranking.is_empty() {
        return vec![HierarchyLevel {
            mesh,
            up_mapping: Vec::new(),
        }];
    }

    ranking.sort_unstable_by(|(_, _, a), (_, _, b)| b.partial_cmp(a).unwrap());

    let mut vertices = Vec::new();
    let mut normals = Vec::new();
    let mut dual_area = Vec::new();
    let mut up_mapping = vec![usize::MAX; mesh.vertices.len()];
    for (i, j, _) in ranking {
        if up_mapping[i] != usize::MAX || up_mapping[j] != usize::MAX {
            continue;
        }

        let ai = mesh.dual_area[i];
        let aj = mesh.dual_area[j];
        let at = ai + aj;

        up_mapping[i] = vertices.len();
        up_mapping[j] = vertices.len();

        vertices.push((ai * mesh.vertices[i] + aj * mesh.vertices[j]) / at);
        normals.push((ai * mesh.normals[i] + aj * mesh.normals[j]).normalize());
        dual_area.push(at);
    }

    for (i, v) in up_mapping
        .iter_mut()
        .enumerate()
        .filter(|(_, v)| **v == usize::MAX)
    {
        *v = vertices.len();
        vertices.push(mesh.vertices[i]);
        normals.push(mesh.normals[i]);
        dual_area.push(mesh.dual_area[i]);
    }

    let mut adjacency_face = vec![Vec::new(); vertices.len()];
    for (i, a) in mesh.adjacency_face.iter().enumerate() {
        let iu = up_mapping[i];
        for (j, _) in a {
            let ju = up_mapping[*j];
            if iu != ju {
                adjacency_face[iu].push((ju, usize::MAX));
            }
        }
    }

    for a in &mut adjacency_face {
        a.sort_unstable_by_key(|(i, _)| *i);
        a.dedup();
    }

    let new_mesh = ProcessMesh {
        vertices,
        normals,
        tris: Vec::new(),
        adjacency_face,
        dual_area,
    };
    let mut up = build(new_mesh);
    up.push(HierarchyLevel { mesh, up_mapping });
    up
}

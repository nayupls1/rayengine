use super::*;
use std::cell::Cell;

fn triangle() -> MeshData {
    MeshData::new(vec![
        Vec3::new(-1.0, -1.0, 0.0),
        Vec3::new(1.0, -1.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    ])
}
fn request(tag: u32, revision: u64) -> MeshUpload<u32> {
    MeshUpload {
        tag,
        revision,
        target: MeshUploadTarget::Create,
        data: triangle(),
    }
}
fn unlimited() -> UploadBudget {
    UploadBudget {
        max_requests: usize::MAX,
        max_bytes: usize::MAX,
        max_time: Duration::MAX,
    }
}
fn reject_upload(_: MeshUploadTarget, _: &MeshData) -> Result<MeshId, Error> {
    Err(Error::Asset("test upload failure".into()))
}

#[test]
fn admission_bounds_count_capacity_bytes_and_retains_rejected_payloads() {
    let mut queue = MeshUploadQueue::new(2, 72).unwrap();
    queue.try_push(request(1, 0)).unwrap();
    queue.try_push(request(2, 0)).unwrap();
    let rejected = queue.try_push(request(3, 0)).unwrap_err();
    assert_eq!(rejected.reason, MeshQueueError::Full);
    assert_eq!(rejected.request.tag, 3);
    assert_eq!(
        (
            queue.len(),
            queue.pending_bytes(),
            queue.front_upload_bytes()
        ),
        (2, 72, Some(60))
    );
    assert_eq!(queue.pop().unwrap().tag, 1);
    queue.try_push(rejected.request).unwrap();
    assert_eq!(queue.pop().unwrap().tag, 2);
    assert_eq!(queue.pop().unwrap().tag, 3);
    assert!(queue.is_empty());
    assert_eq!(queue.pending_bytes(), 0);

    let mut large = request(4, 0);
    large.data.positions.reserve(100);
    let rejected = queue.try_push(large).unwrap_err();
    assert_eq!(rejected.reason, MeshQueueError::MemoryLimit);
    assert_eq!(rejected.request.data.positions.len(), 3);
    let mut invalid = request(5, 0);
    invalid.data.positions.pop();
    assert!(matches!(
        queue.try_push(invalid).unwrap_err().reason,
        MeshQueueError::InvalidMesh(MeshError::IncompleteTriangle)
    ));
    assert!(MeshUploadQueue::<()>::new(0, 1).is_err());
    assert!(MeshUploadQueue::<()>::new(1, 0).is_err());
}

#[test]
fn stale_discards_and_failures_count_against_request_budget_and_bytes_include_uv_fallback() {
    let mut queue = MeshUploadQueue::new(3, 108).unwrap();
    for revision in 0..3 {
        queue.try_push(request(1, revision)).unwrap();
    }
    let mut outcomes = Vec::new();
    let first = queue.process_with_clock(
        UploadBudget {
            max_requests: 1,
            ..unlimited()
        },
        |_, revision| revision > 0,
        |_, _| panic!("stale request uploaded"),
        |result| outcomes.push(result),
        || Duration::ZERO,
    );
    assert_eq!(
        (
            first.processed,
            first.discarded,
            first.attempted,
            first.bytes,
            first.remaining
        ),
        (1, 1, 0, 0, 2)
    );
    assert!(matches!(outcomes[0].outcome, MeshUploadOutcome::Stale));
    let second = queue.process_with_clock(
        UploadBudget {
            max_bytes: 60,
            ..unlimited()
        },
        |_, _| true,
        reject_upload,
        |result| outcomes.push(result),
        || Duration::ZERO,
    );
    assert_eq!(
        (
            second.processed,
            second.attempted,
            second.failed,
            second.bytes,
            second.remaining,
            second.blocked_upload_bytes
        ),
        (1, 1, 1, 60, 1, Some(60))
    );
    assert!(matches!(outcomes[1].outcome, MeshUploadOutcome::Failed(_)));
    assert_eq!(queue.pending_bytes(), 36);
}

#[test]
fn byte_time_and_zero_budgets_retain_requests_without_oversized_uploads() {
    let mut queue = MeshUploadQueue::new(2, 72).unwrap();
    queue.try_push(request(1, 0)).unwrap();
    queue.try_push(request(2, 0)).unwrap();
    for budget in [
        UploadBudget {
            max_requests: 0,
            ..unlimited()
        },
        UploadBudget {
            max_time: Duration::ZERO,
            ..unlimited()
        },
        UploadBudget {
            max_bytes: 59,
            ..unlimited()
        },
    ] {
        let report = queue.process_with_clock(
            budget,
            |_, _| true,
            |_, _| panic!("budget exceeded"),
            |_| panic!("unexpected result"),
            || Duration::ZERO,
        );
        assert_eq!((report.processed, report.remaining), (0, 2));
    }
    let clock = Cell::new(Duration::ZERO);
    let report = queue.process_with_clock(
        UploadBudget {
            max_time: Duration::from_millis(2),
            ..unlimited()
        },
        |_, _| true,
        |target, data| {
            clock.set(Duration::from_millis(3));
            reject_upload(target, data)
        },
        |_| {},
        || clock.get(),
    );
    assert_eq!(
        (
            report.processed,
            report.failed,
            report.remaining,
            report.elapsed
        ),
        (1, 1, 1, Duration::from_millis(3))
    );
}

#[test]
fn gpu_accounting_includes_all_supplied_buffers() {
    let mut data = triangle();
    data.normals = Some(vec![Vec3::Z; 3]);
    data.texcoords = Some(vec![Vec2::ZERO; 3]);
    data.colors = Some(vec![[255; 4]; 3]);
    data.indices = Some(vec![0, 1, 2]);
    assert_eq!(byte_counts(&data), Ok((114, 114)));
}

#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_upload_budget_smoke() {
    use crate::{prelude::*, raylib::prelude::*};
    use rayengine_core::jobs::{JobOutcome, JobPool};
    struct Probe {
        mesh: Option<MeshId>,
        created: Option<MeshId>,
        queue: MeshUploadQueue<u32>,
    }
    fn camera() -> rayengine_core::camera::Camera3D {
        rayengine_core::camera::Camera3D {
            position: Vec3::new(0.0, 0.0, 6.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            vertical_fov: 60.0,
        }
    }
    fn pixel(frame: &Frame<'_, '_>, x: f32) -> Color {
        let mut image = frame.target.texture().load_image().unwrap();
        image.flip_vertical();
        let scale = image.height as f32 / (2.0 * 30_f32.to_radians().tan() * 6.0);
        image.get_color(
            (image.width as f32 * 0.5 + x * scale) as i32,
            image.height / 2,
        )
    }
    impl Game for Probe {
        fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
            self.mesh = Some(ctx.mesh(&triangle())?);
            let pool = JobPool::new(2, 3).unwrap();
            for revision in 1..=3 {
                pool.try_submit(move |_| {
                    let mut data = triangle();
                    if revision == 2 {
                        for vertex in &mut data.positions {
                            vertex.x += 0.75;
                        }
                    }
                    if revision == 3 {
                        for vertex in &mut data.positions {
                            vertex.x += 3.0;
                        }
                    }
                    (revision, data)
                })
                .unwrap();
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            let mut generated = Vec::new();
            while generated.len() < 3 {
                if let Some(completion) = pool.try_recv() {
                    if let JobOutcome::Ready(result) = completion.outcome {
                        generated.push(result);
                    } else {
                        panic!("CPU mesh generation failed");
                    }
                }
                assert!(Instant::now() < deadline, "CPU generation timed out");
                std::thread::yield_now();
            }
            generated.sort_unstable_by_key(|result| result.0);
            for (revision, data) in generated {
                self.queue
                    .try_push(MeshUpload {
                        tag: if revision == 3 { 1 } else { 0 },
                        revision,
                        target: if revision == 3 {
                            MeshUploadTarget::Create
                        } else {
                            MeshUploadTarget::Replace(self.mesh.unwrap())
                        },
                        data,
                    })
                    .unwrap();
            }
            Ok(())
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            let budget = UploadBudget {
                max_requests: 1,
                max_bytes: if frame.index == 1 { 59 } else { 60 },
                max_time: Duration::MAX,
            };
            let mut results = Vec::new();
            let report = frame.upload_meshes(
                &mut self.queue,
                budget,
                |tag, revision| *tag == 1 || revision == 2,
                |result| results.push(result),
            );
            match frame.index {
                0 => assert_eq!(
                    (report.discarded, report.attempted, report.remaining),
                    (1, 0, 2)
                ),
                1 => assert_eq!(
                    (
                        report.attempted,
                        report.blocked_upload_bytes,
                        report.remaining
                    ),
                    (0, Some(60), 2)
                ),
                2 => {
                    assert_eq!(
                        (report.uploaded, report.bytes, report.remaining),
                        (1, 60, 1)
                    );
                    let MeshUploadOutcome::Uploaded(id) = results[0].outcome else {
                        panic!("replacement failed")
                    };
                    assert_eq!(Some(id), self.mesh);
                }
                3 => {
                    assert_eq!((report.uploaded, report.remaining), (1, 0));
                    let MeshUploadOutcome::Uploaded(id) = results[0].outcome else {
                        panic!("creation failed")
                    };
                    self.created = Some(id);
                }
                4 => {
                    frame.assets.unload_mesh(self.created.unwrap());
                    self.queue
                        .try_push(MeshUpload {
                            tag: 1,
                            revision: 4,
                            target: MeshUploadTarget::Replace(self.created.unwrap()),
                            data: triangle(),
                        })
                        .unwrap();
                }
                5 => {
                    assert_eq!((report.failed, report.attempted, report.bytes), (1, 1, 60));
                    assert!(matches!(results[0].outcome, MeshUploadOutcome::Failed(_)));
                }
                _ => unreachable!(),
            }
            frame.clear(Color::BLACK);
            let index = frame.index;
            frame.world_3d(camera(), |canvas| {
                assert!(canvas.mesh(
                    self.mesh.unwrap(),
                    Transform3D::default(),
                    if index < 2 { Color::BLUE } else { Color::GREEN }
                ));
                if index == 3 {
                    assert!(canvas.mesh(self.created.unwrap(), Transform3D::default(), Color::RED));
                }
            });
            assert_eq!(
                pixel(frame, if index < 2 { 0.0 } else { 0.75 }),
                if frame.index < 2 {
                    Color::BLUE
                } else {
                    Color::GREEN
                }
            );
            if index >= 2 {
                assert_eq!(pixel(frame, 0.0), Color::BLACK);
            }
            if frame.index == 3 {
                assert_eq!(pixel(frame, 3.0), Color::RED);
            }
        }
    }
    let mut config = Config::new("budgeted CPU mesh uploads");
    config.window_size = (960, 540);
    config.vsync = false;
    App::new(config)
        .with_options(RunOptions {
            frames: Some(6),
            hidden: true,
            uncapped: true,
            ..RunOptions::default()
        })
        .run(Probe {
            mesh: None,
            created: None,
            queue: MeshUploadQueue::new(3, 1024).unwrap(),
        })
        .unwrap();
}

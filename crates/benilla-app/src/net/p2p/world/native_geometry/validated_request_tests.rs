// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn validation_borrows_exact_objects_and_does_not_transfer_to_changed_inputs() {
    let mut original_session = session();
    let mut original_request = request();
    let replacement = original_session.clone();
    original_session
        .with_validated_request(&original_request, |validated| {
            assert!(std::ptr::eq(validated.session(), &original_session));
            assert!(!std::ptr::eq(validated.session(), &replacement));
            assert!(std::ptr::eq(validated.request(), &original_request));
            assert_eq!(validated.body(), body());
        })
        .unwrap();
    // Mutation requires ending the old borrow and a fresh successful validation.
    original_request.binding.tick = 0;
    assert!(original_session
        .with_validated_request(&original_request, |_| ())
        .is_err());
    original_request = request();
    original_session.geometry[0] ^= 1;
    assert!(original_session
        .with_validated_request(&original_request, |_| ())
        .is_err());
}

#[test]
fn invalid_requests_never_enter_validated_callback() {
    for mutation in 0..7 {
        let (authority, service) = mailbox(session()).unwrap();
        let mut changed = request();
        match mutation {
            0 => changed.binding.world[0] ^= 1,
            1 => changed.binding.profile[0] ^= 1,
            2 => changed.binding.geometry[0] ^= 1,
            3 => changed.binding.tick = 0,
            4 => changed.binding.npc = None,
            5 => changed.binding.authority_policy[0] ^= 1,
            6 => {
                let Query::Movement(query) = &mut changed.query else {
                    unreachable!()
                };
                query.map_id += 1;
            }
            _ => unreachable!(),
        }
        let (reply, rx) = bounded(1);
        authority
            .tx
            .try_send(Work::new(
                1,
                changed,
                Instant::now() + WAIT,
                reply,
                service.active.clone(),
            ))
            .unwrap();
        assert_eq!(
            service.serve_validated_queries_until(1, |_, _| {
                panic!("invalid request reached provider: {mutation}")
            }),
            1
        );
        assert_eq!(rx.try_recv().unwrap().result, Err(Failure::Binding));
    }
}

#[test]
fn validated_callback_preserves_non_route_facts_and_cancellation_fences() {
    let (mut authority, service) = mailbox(session()).unwrap();
    let worker = std::thread::spawn(move || authority.resolve(&request()));
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut calls = 0;
    while !worker.is_finished() && Instant::now() < deadline {
        service.serve_validated_queries_until(1, |validated, _| {
            calls += 1;
            assert!(std::ptr::eq(validated.session(), &service.session));
            let Query::Movement(movement) = &validated.request().query else {
                unreachable!()
            };
            solve(movement, validated.body(), &mut Proof(Ok(()))).map(Facts::Movement)
        });
        std::thread::yield_now();
    }
    let reply = worker.join().unwrap().unwrap();
    assert_eq!(calls, 1);
    assert_eq!(reply.request, request());
    assert_eq!(
        reply.facts,
        Facts::Movement(solve(&movement(), body(), &mut Proof(Ok(()))).unwrap())
    );

    let (authority, service) = mailbox(session()).unwrap();
    let (reply, rx) = bounded(1);
    authority
        .tx
        .try_send(Work::new(
            1,
            request(),
            Instant::now() + WAIT,
            reply,
            service.active.clone(),
        ))
        .unwrap();
    service.serve_validated_queries_until(1, |validated, _| {
        service.active.store(false, Ordering::Release);
        let Query::Movement(movement) = &validated.request().query else {
            unreachable!()
        };
        solve(movement, validated.body(), &mut Proof(Ok(()))).map(Facts::Movement)
    });
    assert!(rx.try_recv().is_err(), "cancelled attempt published facts");
}

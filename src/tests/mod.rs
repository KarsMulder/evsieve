mod framework;
pub use framework::run_test;

#[test]
fn rudimentary_test() {
    run_test(
        // Arguments
        "--map key:a key:b",
        // Input
        "key:a:1 key:c:1 key:a:0 key:c:0",
        // Output
        "key:b:1 key:c:1 key:b:0 key:c:0",
    )
}

#[test]
fn test_withhold_with_two_hooks() {
    run_test(
        // Arguments
        "
        --hook key:a key:b
        --hook key:a key:c
        --withhold
        ",
        // Input
        "
        key:a:1 key:a:0
        key:a:1 key:z:1 key:a:0 key:z:0
        key:a:1 key:z:1 key:z:0 key:a:0

        key:x:1 key:a:1 key:b:1 key:a:0 key:b:0 key:x:0
        key:y:1 key:b:1 key:a:1 key:a:0 key:b:0 key:y:0
        key:z:1 key:a:1 key:c:1 key:a:0 key:c:0 key:z:0
        key:w:1 key:b:1 key:c:1 key:b:0 key:c:0 key:w:0

        key:x:1 key:a:1 key:b:1 key:c:1 key:a:0 key:b:0 key:c:0 key:x:0
        key:y:1 key:c:1 key:a:1 key:b:1 key:c:0 key:a:0 key:b:0 key:y:0

        ",
        // Output
        "
        key:a:1 key:a:0
        key:z:1 key:a:1 key:a:0 key:z:0
        key:z:1 key:z:0 key:a:1 key:a:0

        key:x:1 key:x:0
        key:y:1 key:y:0
        key:z:1 key:z:0
        key:w:1 key:b:1 key:b:0 key:c:1 key:c:0 key:w:0

        key:x:1 key:x:0
        key:y:1 key:y:0
        "
    )
}

#[test]
fn test_withhold_with_three_trackers() {
    run_test(
        // Arguments
        "
        --hook key:a key:b key:c
        --withhold
        ",
        // Input
        "
        key:a:1 key:a:0
        key:a:1 key:z:1 key:a:0 key:z:0

        key:a:1 key:a:0 key:b:1 key:b:0
        key:a:1 key:b:1 key:a:0 key:b:0
        key:b:1 key:a:1 key:b:0 key:a:0

        key:x:1 key:a:1 key:b:1 key:c:1 key:a:0 key:b:0 key:c:0 key:x:0
        key:y:1 key:b:1 key:c:1 key:a:1 key:b:0 key:c:0 key:a:0 key:y:0
        key:z:1 key:c:1 key:a:1 key:b:1 key:c:0 key:a:0 key:b:0 key:z:0

        key:x:1
            key:a:1 key:c:1 key:a:0 key:b:1 key:c:0 key:a:1 key:c:1 key:a:0 key:b:0 key:c:0
        key:x:0
        ",
        // Output
        "
        key:a:1 key:a:0
        key:z:1 key:a:1 key:a:0 key:z:0

        key:a:1 key:a:0 key:b:1 key:b:0
        key:a:1 key:a:0 key:b:1 key:b:0
        key:b:1 key:b:0 key:a:1 key:a:0

        key:x:1 key:x:0
        key:y:1 key:y:0
        key:z:1 key:z:0

        key:x:1
            key:a:1 key:a:0 key:c:1 key:c:0
        key:x:0
        "
    )
}

#[test]
fn test_withhold_with_breaks_on() {
    run_test(
        // Arguments
        "
        --hook key:a key:b key:c breaks-on=key:q:1 send-key=key:w
        --withhold
        ",
        // Input
        "
        key:a:1 key:c:1 key:a:0 key:c:0
        key:a:1 key:c:1 key:c:0 key:a:0
        key:a:1 key:c:1 key:b:1 key:b:0 key:c:0 key:a:0

        key:a:1 key:c:1 key:z:1 key:a:0 key:c:0 key:z:0
        key:a:1 key:c:1 key:q:1 key:a:0 key:c:0 key:q:0
        key:a:1 key:c:1 key:q:1 key:q:0 key:a:0 key:c:0
        key:a:1 key:c:1 key:q:1 key:b:1 key:q:0 key:b:0 key:a:0 key:c:0

        ",
        // Output
        "
        key:a:1 key:a:0 key:c:1 key:c:0
        key:c:1 key:c:0 key:a:1 key:a:0
        key:w:1 key:w:0

        key:z:1 key:a:1 key:a:0 key:c:1 key:c:0 key:z:0
        key:a:1 key:c:1 key:q:1 key:a:0 key:c:0 key:q:0
        key:a:1 key:c:1 key:q:1 key:q:0 key:a:0 key:c:0
        key:a:1 key:c:1 key:q:1 key:q:0 key:b:1 key:b:0 key:a:0 key:c:0

        "
    )
}

#[test]
fn test_withhold_sending_its_own_keys() {
    run_test(
        // Arguments
        "
        --hook key:a key:b send-key=key:a
        --withhold
        ",
        // Input
        "
        key:a:1 key:a:0
        key:b:1 key:b:0
        key:x:1 key:x:0

        key:a:1 key:b:1 key:a:0 key:b:0
        key:y:1 key:y:0
        key:b:1 key:a:1 key:b:0 key:a:0
        key:y:1 key:y:0
        ",
        // Output
        "
        key:a:1 key:a:0
        key:b:1 key:b:0
        key:x:1 key:x:0

        key:a:1 key:a:0
        key:y:1 key:y:0
        key:a:1 key:a:0
        key:y:1 key:y:0
        "
    )
}

#[test]
fn test_withhold_sending_keys_for_later_hooks() {
    run_test(
        // Arguments
        "
        --hook key:a key:b send-key=key:s
        --hook key:s key:t send-key=key:f
        --withhold
        ",
        // Input
        "
        key:s:1 key:s:0
        key:s:1 key:t:1 key:s:0 key:t:0
        key:x:1 key:x:0

        key:s:1 key:a:1 key:s:0 key:a:0
        key:y:1 key:y:0

        key:s:1 key:a:1 key:b:1 key:s:0 key:a:0 key:b:0
        key:z:1 key:z:0
        key:t:1 key:a:1 key:b:1 key:t:0 key:a:0 key:b:0
        key:z:1 key:z:0

        key:x:1 key:a:1 key:b:1 key:x:0 key:a:0 key:b:0
        key:y:1 key:y:0

        key:s:1 key:a:1 key:b:1 key:t:1 key:s:0 key:a:0 key:b:0 key:t:0
        key:z:1 key:z:0
        ",
        // Output
        "
        key:s:1 key:s:0
        key:f:1 key:f:0
        key:x:1 key:x:0

        key:s:1 key:s:0 key:a:1 key:a:0
        key:y:1 key:y:0

        key:s:1 key:s:1 key:s:0 key:s:0
        key:z:1 key:z:0
        key:f:1 key:f:0
        key:z:1 key:z:0

        key:x:1 key:x:0 key:s:1 key:s:0
        key:y:1 key:y:0

        key:f:1 key:f:0
        key:z:1 key:z:0
        "
    )
}

#[test]
fn test_withhold_sending_keys_for_later_hooks_2() {
    run_test(
        // Arguments
        "
        --hook key:a key:b
        --hook key:s send-key=key:a
        --hook key:a key:c send-key=key:f
        --withhold
        ",
        // Input
        "
        key:a:1 key:a:0 key:x:1 key:x:0
        key:b:1 key:b:0 key:x:1 key:x:0
        key:c:1 key:c:0 key:x:1 key:x:0
        key:s:1 key:s:0 key:x:1 key:x:0
        
        key:s:1 key:c:1 key:s:0 key:c:0 key:x:1 key:x:0
        key:c:1 key:s:1 key:c:0 key:s:0 key:y:1 key:y:0
        key:s:1 key:b:1 key:s:0 key:b:0 key:x:1 key:x:0
        key:b:1 key:s:1 key:b:0 key:s:0 key:y:1 key:y:0
        key:a:1 key:b:1 key:a:0 key:b:0 key:x:1 key:x:0
        key:b:1 key:a:1 key:b:0 key:a:0 key:y:1 key:y:0

        key:a:1 key:b:1 key:c:1 key:a:0 key:b:0 key:c:0
        key:a:1 key:a:0
        ",
        // Output
        "
        key:a:1 key:a:0 key:x:1 key:x:0
        key:b:1 key:b:0 key:x:1 key:x:0
        key:c:1 key:c:0 key:x:1 key:x:0
        key:a:1 key:a:0 key:x:1 key:x:0

        key:f:1 key:f:0 key:x:1 key:x:0
        key:f:1 key:f:0 key:y:1 key:y:0
        key:a:1 key:a:0 key:b:1 key:b:0 key:x:1 key:x:0
        key:b:1 key:b:0 key:a:1 key:a:0 key:y:1 key:y:0
        key:x:1 key:x:0
        key:y:1 key:y:0

        key:f:1 key:f:0
        key:a:1 key:a:0
        "
    )
}

#[test]
fn test_withhold_breaks_on() {
    run_test(
        // Arguments
        "
        --hook btn:left  send-key=key:d
        --hook btn:right
        --hook key:a key:b sequential breaks-on=key::1 send-key=key:c
        --hook key:e send-key=key:f
        --withhold
        ",
        // Input
        "
        key:a:1 rel:x:1 key:a:0
        key:b:1 rel:x:2 key:b:0
        key:c:1 rel:x:3 key:c:0
        key:e:1 rel:x:4 key:e:0
        btn:left:1  rel:x:5 btn:left:0
        btn:right:1 rel:x:6 btn:right:0

        key:a:1 key:x:1 key:a:0 key:x:0
        key:a:1 btn:c:1 key:a:0 btn:c:0
        key:b:1 key:x:1 key:b:0 key:x:0
        key:b:1 btn:c:1 key:b:0 btn:c:0
        
        key:a:1 btn:left:1 key:a:0 btn:left:0
        key:a:1 btn:left:1 key:b:1 key:a:0 btn:left:0 key:b:0
        key:a:1 btn:right:1 key:a:0 btn:right:0
        key:a:1 btn:right:1 key:b:1 key:a:0 btn:right:0 key:b:0
        
        ",
        // Output
        "
        rel:x:1 key:a:1 key:a:0
        key:b:1 rel:x:2 key:b:0
        key:c:1 rel:x:3 key:c:0
        key:f:1 rel:x:4 key:f:0
        key:d:1 rel:x:5 key:d:0
        rel:x:6

        key:a:1 key:x:1 key:a:0 key:x:0
        btn:c:1 key:a:1 key:a:0 btn:c:0
        key:b:1 key:x:1 key:b:0 key:x:0
        key:b:1 btn:c:1 key:b:0 btn:c:0

        key:a:1 key:d:1 key:a:0 key:d:0
        key:a:1 key:d:1 key:b:1 key:a:0 key:d:0 key:b:0
        key:a:1 key:a:0
        key:c:1 key:c:0
        "
    )
}

#[test]
fn test_withhold_for_channelless_hooks() {
    // The supposed outcome may look unintuitive, but it is the correct one because a tracker
    // of a hook is documented to deactivate when an event with a value not in the range 1~
    // arrives. That means that the key:a tracker gets deactivated upon key:a:0@foo and therefore
    // the key:a:1@bar event can be immediately released as well.
    //
    // These are stupid semantics which are only retained for backwards compatibility. In a
    // hypothetical evsieve 2.0, they should be fixed.
    run_test(
        // Arguments
        "
        --hook key:a key:b
        --withhold
        ",
        // Input
        "
        key:a:1@foo key:a:1@bar key:z:1 key:a:0@foo key:z:0 key:a:0@bar
        ",
        // Output
        "
        key:z:1 key:a:1@foo key:a:1@bar key:a:0@foo key:z:0 key:a:0@bar
        "
    )
}

#[test]
fn test_abs_to_rel_passthrough() {
    run_test(
        // Arguments
        "--abs-to-rel abs:x rel:x",
        // Input
        "key:a:1 abs:y:100 key:b:0",
        // Output
        "key:a:1 abs:y:100 key:b:0",
    )
}

#[test]
fn test_abs_to_rel_first_event() {
    // First event on a channel has no prior state, so the output rel value is 0.
    run_test(
        // Arguments
        "--abs-to-rel abs:x rel:x",
        // Input
        "abs:x:100",
        // Output
        "rel:x:0",
    )
}

#[test]
fn test_abs_to_rel_sequence() {
    // Subsequent events produce the delta between the current and the previously observed abs value.
    run_test(
        // Arguments
        "--abs-to-rel abs:x rel:x",
        // Input
        "abs:x:0 abs:x:50 abs:x:30",
        // Output
        "rel:x:0 rel:x:50 rel:x:-20",
    )
}

#[test]
fn test_abs_to_rel_speed() {
    run_test(
        // Arguments
        "
        --abs-to-rel abs:x rel:x speed=2
        --abs-to-rel abs:y rel:y speed=0.5
        ",
        // Input
        "abs:x:0 abs:x:50 abs:y:0 abs:y:50",
        // Output
        "rel:x:0 rel:x:100 rel:y:0 rel:y:25",
    )
}


#[test]
fn test_abs_to_rel_composition() {
    run_test(
        // Arguments
        "
        --block abs:x:50
        --abs-to-rel abs:x rel:x
        ",
        // Input
        "abs:x:0 abs:x:50 abs:x:100",
        // Output
        "rel:x:0 rel:x:100",
    )
}

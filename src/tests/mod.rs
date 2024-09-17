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

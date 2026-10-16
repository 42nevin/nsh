//! The codes of the native animation graph's operations (`ANIM_*`), as
//! `nsh/animated` encodes them for its decoder (`nsh-js` `protocol.rs`).

codes! { ANIM:
    /// node, value
    ANIM_VALUE = 0,
    /// node, input, inputRange, outputRange, extrapolateLeft, extrapolateRight, easing
    ANIM_INTERPOLATE = 1,
    /// node, op, a, b
    ANIM_MATH = 2,
    /// node, value
    ANIM_SET = 3,
    /// node, animation id, config
    ANIM_START = 4,
    /// node
    ANIM_STOP = 5,
    /// node
    ANIM_RELEASE = 6,
    /// node, input, min, max
    ANIM_DIFF_CLAMP = 7,
    /// animation id, composite key, spec
    ANIM_START_COMPOSITE = 8,
    /// animation id
    ANIM_STOP_ANIMATION = 9,
    /// composite key
    ANIM_RESET_COMPOSITE = 10,
    /// composite key
    ANIM_RELEASE_COMPOSITE = 11,
}

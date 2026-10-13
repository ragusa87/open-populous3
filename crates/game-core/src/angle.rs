//! Integer angles for the simulation: a turn is `TURN` steps, sines are fixed point over `ONE` (no floats in state).

/// Angle steps in a full turn.
pub const TURN: u32 = 2048;
/// 1.0 in the fixed point of `sin` and `cos`.
pub const ONE: i32 = 4096;

/// `ONE × sin` for each step of a quarter turn, both ends included.
const QUARTER: [i16; 513] = [
    0, 13, 25, 38, 50, 63, 75, 88, 101, 113, 126, 138, 151, 163, 176, 188,
    201, 214, 226, 239, 251, 264, 276, 289, 301, 314, 326, 339, 351, 364, 376, 389,
    401, 414, 426, 439, 451, 464, 476, 489, 501, 514, 526, 539, 551, 564, 576, 589,
    601, 613, 626, 638, 651, 663, 675, 688, 700, 713, 725, 737, 750, 762, 774, 787,
    799, 811, 824, 836, 848, 861, 873, 885, 897, 910, 922, 934, 946, 959, 971, 983,
    995, 1007, 1020, 1032, 1044, 1056, 1068, 1080, 1092, 1105, 1117, 1129, 1141, 1153, 1165, 1177,
    1189, 1201, 1213, 1225, 1237, 1249, 1261, 1273, 1285, 1297, 1309, 1321, 1332, 1344, 1356, 1368,
    1380, 1392, 1404, 1415, 1427, 1439, 1451, 1462, 1474, 1486, 1498, 1509, 1521, 1533, 1544, 1556,
    1567, 1579, 1591, 1602, 1614, 1625, 1637, 1648, 1660, 1671, 1683, 1694, 1706, 1717, 1729, 1740,
    1751, 1763, 1774, 1785, 1797, 1808, 1819, 1830, 1842, 1853, 1864, 1875, 1886, 1898, 1909, 1920,
    1931, 1942, 1953, 1964, 1975, 1986, 1997, 2008, 2019, 2030, 2041, 2052, 2062, 2073, 2084, 2095,
    2106, 2117, 2127, 2138, 2149, 2159, 2170, 2181, 2191, 2202, 2213, 2223, 2234, 2244, 2255, 2265,
    2276, 2286, 2296, 2307, 2317, 2328, 2338, 2348, 2359, 2369, 2379, 2389, 2399, 2410, 2420, 2430,
    2440, 2450, 2460, 2470, 2480, 2490, 2500, 2510, 2520, 2530, 2540, 2550, 2559, 2569, 2579, 2589,
    2598, 2608, 2618, 2628, 2637, 2647, 2656, 2666, 2675, 2685, 2694, 2704, 2713, 2723, 2732, 2741,
    2751, 2760, 2769, 2779, 2788, 2797, 2806, 2815, 2824, 2833, 2843, 2852, 2861, 2870, 2878, 2887,
    2896, 2905, 2914, 2923, 2932, 2940, 2949, 2958, 2967, 2975, 2984, 2992, 3001, 3009, 3018, 3026,
    3035, 3043, 3052, 3060, 3068, 3077, 3085, 3093, 3102, 3110, 3118, 3126, 3134, 3142, 3150, 3158,
    3166, 3174, 3182, 3190, 3198, 3206, 3214, 3221, 3229, 3237, 3244, 3252, 3260, 3267, 3275, 3282,
    3290, 3297, 3305, 3312, 3320, 3327, 3334, 3342, 3349, 3356, 3363, 3370, 3378, 3385, 3392, 3399,
    3406, 3413, 3420, 3426, 3433, 3440, 3447, 3454, 3461, 3467, 3474, 3481, 3487, 3494, 3500, 3507,
    3513, 3520, 3526, 3532, 3539, 3545, 3551, 3558, 3564, 3570, 3576, 3582, 3588, 3594, 3600, 3606,
    3612, 3618, 3624, 3630, 3636, 3642, 3647, 3653, 3659, 3664, 3670, 3675, 3681, 3686, 3692, 3697,
    3703, 3708, 3713, 3719, 3724, 3729, 3734, 3739, 3745, 3750, 3755, 3760, 3765, 3770, 3775, 3779,
    3784, 3789, 3794, 3798, 3803, 3808, 3812, 3817, 3822, 3826, 3831, 3835, 3839, 3844, 3848, 3852,
    3857, 3861, 3865, 3869, 3873, 3877, 3881, 3885, 3889, 3893, 3897, 3901, 3905, 3909, 3912, 3916,
    3920, 3923, 3927, 3930, 3934, 3937, 3941, 3944, 3948, 3951, 3954, 3958, 3961, 3964, 3967, 3970,
    3973, 3976, 3979, 3982, 3985, 3988, 3991, 3994, 3996, 3999, 4002, 4005, 4007, 4010, 4012, 4015,
    4017, 4020, 4022, 4024, 4027, 4029, 4031, 4034, 4036, 4038, 4040, 4042, 4044, 4046, 4048, 4050,
    4052, 4053, 4055, 4057, 4059, 4060, 4062, 4064, 4065, 4067, 4068, 4070, 4071, 4072, 4074, 4075,
    4076, 4077, 4079, 4080, 4081, 4082, 4083, 4084, 4085, 4086, 4087, 4088, 4088, 4089, 4090, 4090,
    4091, 4092, 4092, 4093, 4093, 4094, 4094, 4094, 4095, 4095, 4095, 4096, 4096, 4096, 4096, 4096,
    4096
];

/// `ONE × sin(angle)`, the angle in `TURN` steps (any value: it wraps).
pub fn sin(angle: u32) -> i32 {
    let a = angle % TURN;
    let quarter = TURN / 4;
    match a / quarter {
        0 => QUARTER[a as usize] as i32,
        1 => QUARTER[(2 * quarter - a) as usize] as i32,
        2 => -(QUARTER[(a - 2 * quarter) as usize] as i32),
        _ => -(QUARTER[(TURN - a) as usize] as i32),
    }
}

/// `ONE × cos(angle)`.
pub fn cos(angle: u32) -> i32 {
    sin(angle + TURN / 4)
}

/// `(x, z)` turned by `angle` (from +x towards +z).
pub fn rotate((x, z): (i32, i32), angle: u32) -> (i32, i32) {
    let (s, c) = (sin(angle), cos(angle));
    ((x * c - z * s) / ONE, (x * s + z * c) / ONE)
}

/// An angle in `TURN` steps, wrapping: a heading or a tilt held in the simulation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Angle(u16);

impl Angle {
    pub const ZERO: Angle = Angle(0);

    /// `steps` of `TURN` (wrapped).
    pub const fn new(steps: u32) -> Angle {
        Angle((steps % TURN) as u16)
    }

    pub const fn steps(self) -> u32 {
        self.0 as u32
    }

    /// Turned by `by` steps, either way.
    pub fn turned(self, by: i32) -> Angle {
        Angle::new((self.0 as i32 + by).rem_euclid(TURN as i32) as u32)
    }

    /// Its signed size, `-TURN / 2..TURN / 2`: how far it turns from 0, and which way.
    pub fn signed(self) -> i32 {
        let a = self.0 as i32;
        if a >= TURN as i32 / 2 { a - TURN as i32 } else { a }
    }
}

/// The angle of `(x, z)` (from +x towards +z, `rotate`'s sense); 0 for the zero vector.
pub fn atan2(z: i32, x: i32) -> Angle {
    let (ax, az) = (x.unsigned_abs() as i64, z.unsigned_abs() as i64);
    let (mut lo, mut hi) = (0u32, TURN / 4);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if sin(mid) as i64 * ax >= cos(mid) as i64 * az {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Angle::new(match (x >= 0, z >= 0) {
        (true, true) => lo,
        (false, true) => TURN / 2 - lo,
        (false, false) => TURN / 2 + lo,
        (true, false) => TURN - lo,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_of_a_vector() {
        assert_eq!([atan2(0, 100), atan2(100, 0), atan2(0, -100), atan2(-100, 0)].map(Angle::steps), [0, 512, 1024, 1536]);
        assert_eq!(atan2(100, 100).steps(), 256);
        assert_eq!(atan2(-100, -100).steps(), 1280);
        assert_eq!(atan2(0, 0), Angle::ZERO);
        for a in (0..TURN).step_by(37) {
            let (x, z) = rotate((10_000, 0), a);
            assert!(atan2(z, x).turned(-(a as i32)).signed().abs() <= 1, "{a}");
        }
    }

    #[test]
    fn angles_wrap_and_turn_either_way() {
        assert_eq!(Angle::new(TURN + 5).steps(), 5);
        assert_eq!(Angle::new(10).turned(-20).steps(), TURN - 10);
        assert_eq!(Angle::new(TURN - 10).signed(), -10);
        assert_eq!(Angle::new(398).signed(), 398);
    }

    #[test]
    fn quarter_turns() {
        assert_eq!([0, 512, 1024, 1536, 2048].map(sin), [0, ONE, 0, -ONE, 0]);
        assert_eq!([0, 512, 1024, 1536].map(cos), [ONE, 0, -ONE, 0]);
        assert_eq!(sin(256), 2896, "45°");
        assert_eq!(sin(256 + 1024), -2896);
    }

    #[test]
    fn turning_a_vector() {
        assert_eq!(rotate((100, 0), 512), (0, 100));
        assert_eq!(rotate((100, 0), 1024), (-100, 0));
        assert_eq!(rotate((0, 100), 512), (-100, 0));
    }
}

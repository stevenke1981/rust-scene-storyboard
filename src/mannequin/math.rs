//! Minimal 3D vector / rotation-matrix math (no external dependency).

use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn v3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3 { x, y, z }
}

impl Vec3 {
    pub const ZERO: Vec3 = v3(0.0, 0.0, 0.0);
    pub const X: Vec3 = v3(1.0, 0.0, 0.0);
    pub const Y: Vec3 = v3(0.0, 1.0, 0.0);
    pub const Z: Vec3 = v3(0.0, 0.0, 1.0);

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: Vec3) -> Vec3 {
        v3(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x)
    }
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }
    /// Unit vector, or `fallback` if the vector is (nearly) zero.
    pub fn normalized_or(self, fallback: Vec3) -> Vec3 {
        let l = self.length();
        if l > 1e-6 { self * (1.0 / l) } else { fallback }
    }
    pub fn normalized(self) -> Vec3 {
        self.normalized_or(Vec3::Y)
    }
    pub fn lerp(self, o: Vec3, t: f32) -> Vec3 {
        self + (o - self) * t
    }
    /// Component of `self` perpendicular to the unit vector `axis`.
    pub fn reject(self, axis: Vec3) -> Vec3 {
        self - axis * self.dot(axis)
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
    pub fn from_array(a: [f32; 3]) -> Vec3 {
        v3(a[0], a[1], a[2])
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        *self = *self + o;
    }
}
impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        v3(self.x * s, self.y * s, self.z * s)
    }
}
impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        v3(-self.x, -self.y, -self.z)
    }
}

/// 3x3 matrix stored as columns. Used for rotations (orthonormal) and for
/// ellipsoid axes (rotation * scale).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat3 {
    pub x: Vec3,
    pub y: Vec3,
    pub z: Vec3,
}

impl Default for Mat3 {
    fn default() -> Self {
        Mat3::IDENTITY
    }
}

impl Mat3 {
    pub const IDENTITY: Mat3 = Mat3 { x: Vec3::X, y: Vec3::Y, z: Vec3::Z };

    pub fn from_cols(x: Vec3, y: Vec3, z: Vec3) -> Mat3 {
        Mat3 { x, y, z }
    }
    pub fn mul_vec(&self, v: Vec3) -> Vec3 {
        self.x * v.x + self.y * v.y + self.z * v.z
    }
    pub fn mul(&self, o: &Mat3) -> Mat3 {
        Mat3 { x: self.mul_vec(o.x), y: self.mul_vec(o.y), z: self.mul_vec(o.z) }
    }
    pub fn transpose(&self) -> Mat3 {
        Mat3 {
            x: v3(self.x.x, self.y.x, self.z.x),
            y: v3(self.x.y, self.y.y, self.z.y),
            z: v3(self.x.z, self.y.z, self.z.z),
        }
    }
    /// Scale the columns (i.e. `self * diag(s)`).
    pub fn scale_cols(&self, s: Vec3) -> Mat3 {
        Mat3 { x: self.x * s.x, y: self.y * s.y, z: self.z * s.z }
    }
    pub fn rot_x(a: f32) -> Mat3 {
        let (s, c) = a.sin_cos();
        Mat3 { x: Vec3::X, y: v3(0.0, c, s), z: v3(0.0, -s, c) }
    }
    pub fn rot_y(a: f32) -> Mat3 {
        let (s, c) = a.sin_cos();
        Mat3 { x: v3(c, 0.0, -s), y: Vec3::Y, z: v3(s, 0.0, c) }
    }
    pub fn rot_z(a: f32) -> Mat3 {
        let (s, c) = a.sin_cos();
        Mat3 { x: v3(c, s, 0.0), y: v3(-s, c, 0.0), z: Vec3::Z }
    }
    /// Rotation about a unit axis (Rodrigues).
    pub fn axis_angle(axis: Vec3, a: f32) -> Mat3 {
        let k = axis.normalized_or(Vec3::Y);
        let (s, c) = a.sin_cos();
        let t = 1.0 - c;
        let col = |e: Vec3| e * c + k.cross(e) * s + k * (k.dot(e) * t);
        Mat3 { x: col(Vec3::X), y: col(Vec3::Y), z: col(Vec3::Z) }
    }
    /// Smallest rotation taking direction `a` onto direction `b`.
    pub fn rotation_between(a: Vec3, b: Vec3) -> Mat3 {
        let a = a.normalized();
        let b = b.normalized();
        let d = a.dot(b).clamp(-1.0, 1.0);
        let axis = a.cross(b);
        if axis.length() < 1e-6 {
            if d > 0.0 {
                return Mat3::IDENTITY;
            }
            let perp = if a.x.abs() < 0.9 { Vec3::X } else { Vec3::Z };
            return Mat3::axis_angle(a.cross(perp), std::f32::consts::PI);
        }
        Mat3::axis_angle(axis, d.acos())
    }
    /// Rotation whose local +Y maps to `up` and local +Z maps (as closely as
    /// possible) to `fwd`.
    pub fn look(up: Vec3, fwd: Vec3) -> Mat3 {
        let y = up.normalized();
        let mut z = fwd.reject(y);
        if z.length() < 1e-4 {
            // fwd parallel to up: pick any perpendicular direction.
            z = if y.z.abs() < 0.9 { Vec3::Z.reject(y) } else { Vec3::X.reject(y) };
        }
        let z = z.normalized();
        let x = y.cross(z);
        Mat3 { x, y, z }
    }
    /// Euler angles in degrees, applied as `Rz * Ry * Rx` (X first).
    pub fn from_euler_deg(e: [f32; 3]) -> Mat3 {
        let [x, y, z] = e.map(f32::to_radians);
        Mat3::rot_z(z).mul(&Mat3::rot_y(y)).mul(&Mat3::rot_x(x))
    }
    /// Inverse of [`Mat3::from_euler_deg`].
    pub fn to_euler_deg(&self) -> [f32; 3] {
        // Row/col access: m(r, c) = column c, component r.
        let r20 = self.x.z;
        let r21 = self.y.z;
        let r22 = self.z.z;
        let r10 = self.x.y;
        let r00 = self.x.x;
        let beta = (-r20).clamp(-1.0, 1.0).asin();
        let (alpha, gamma) = if r20.abs() < 0.99999 {
            (r21.atan2(r22), r10.atan2(r00))
        } else {
            // Gimbal lock: put everything into X.
            let r01 = self.y.x;
            let r11 = self.y.y;
            // With gamma = 0: column y = (sin(a)*sin(b), cos(a), 0) and sin(b) = -r20.
            ((r01 * (-r20).signum()).atan2(r11), 0.0)
        };
        let out = [alpha.to_degrees(), beta.to_degrees(), gamma.to_degrees()];
        out.map(|a| if a.is_finite() { a } else { 0.0 })
    }
    /// Re-orthonormalise (Gram-Schmidt on Y then Z).
    pub fn orthonormalized(&self) -> Mat3 {
        Mat3::look(self.y, self.z)
    }
    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

/// Wrap an angle in degrees to (-180, 180].
pub fn wrap_deg(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a > 180.0 {
        a -= 360.0;
    } else if a <= -180.0 {
        a += 360.0;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn euler_round_trip() {
        for e in [[10.0, 20.0, 30.0], [-80.0, 45.0, 170.0], [0.0, -60.0, 0.0], [120.0, 10.0, -100.0]] {
            let m = Mat3::from_euler_deg(e);
            let back = Mat3::from_euler_deg(m.to_euler_deg());
            assert!(close(m.x, back.x) && close(m.y, back.y) && close(m.z, back.z), "{e:?}");
        }
    }

    #[test]
    fn gimbal_lock_round_trip() {
        for e in [[30.0, 90.0, 0.0], [-40.0, -90.0, 0.0]] {
            let m = Mat3::from_euler_deg(e);
            let back = Mat3::from_euler_deg(m.to_euler_deg());
            assert!(close(m.x, back.x) && close(m.y, back.y) && close(m.z, back.z), "{e:?} -> {:?}", m.to_euler_deg());
        }
    }

    #[test]
    fn rotation_between_maps_vectors() {
        let a = v3(0.0, -1.0, 0.0);
        for b in [v3(1.0, 0.0, 0.0), v3(0.3, 0.5, -0.8), v3(0.0, 1.0, 0.0)] {
            let r = Mat3::rotation_between(a, b);
            assert!(close(r.mul_vec(a), b.normalized()));
        }
    }

    #[test]
    fn look_is_orthonormal() {
        let m = Mat3::look(v3(0.2, 1.0, 0.1), v3(0.0, 0.3, 1.0));
        assert!((m.x.length() - 1.0).abs() < 1e-5);
        assert!(m.x.dot(m.y).abs() < 1e-5 && m.y.dot(m.z).abs() < 1e-5);
        assert!(close(m.x.cross(m.y), m.z));
    }
}

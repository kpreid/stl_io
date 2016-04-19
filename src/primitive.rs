use std::fmt::Debug;
use std::f64;

use Float;

use types::{Point, Vector, Transform, EPSILON_X, EPSILON_Y, EPSILON_Z};

pub trait ImplicitFunction {
    fn value(&self, p: &Point) -> Float;
    fn normal(&self, p: &Point) -> Vector;
}

fn normal_from_implicit<T: ImplicitFunction>(f: &T, p: &Point) -> Vector {
    let center = f.value(p);
    let dx = f.value(&(*p + EPSILON_X)) - center;
    let dy = f.value(&(*p + EPSILON_Y)) - center;
    let dz = f.value(&(*p + EPSILON_Z)) - center;
    Vector::new(dx, dy, dz).normalize()
}

pub trait Primitive: ImplicitFunction + Clone + Debug {}

pub trait Object: ImplicitFunction + ObjectClone {
    fn apply_transform(&mut self, other: &Transform);
    fn translate(&mut self, t: Vector) {
        let trans = Transform::translate(&t);
        self.apply_transform(&trans);
    }
    fn rotate(&mut self, r: Vector) {
        let trans = Transform::rotate(&r);
        self.apply_transform(&trans);
    }
    fn scale(&mut self, s: Float) {
        let trans = Transform::scale(s);
        self.apply_transform(&trans);
    }
    fn to_string(&self) -> String {
        format!("{:?}", self)
    }
}

pub trait ObjectClone {
    fn clone_box(&self) -> Box<Object>;
}

impl<T> ObjectClone for T
    where T: 'static + Object + Clone
{
    fn clone_box(&self) -> Box<Object> {
        Box::new(self.clone())
    }
}

// We can now implement Clone manually by forwarding to clone_box.
impl Clone for Box<Object> {
    fn clone(&self) -> Box<Object> {
        self.clone_box()
    }
}

// TODO: This is a hack. Replace it with something sane.
impl PartialEq for Box<Object> {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

// TODO: This is a hack. Replace it with something sane.
impl PartialOrd for Box<Object> {
    fn partial_cmp(&self, other: &Self) -> Option<::std::cmp::Ordering> {
        let s = self.to_string();
        let o = other.to_string();
        if s < o {
            return Some(::std::cmp::Ordering::Less);
        } else if s > o {
            return Some(::std::cmp::Ordering::Greater);
        } else {
            return Some(::std::cmp::Ordering::Equal);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PrimitiveWrapper<T: Primitive> {
    primitive: Box<T>,
    transform: Transform,
}

impl<T: Primitive + 'static> ImplicitFunction for PrimitiveWrapper<T> {
    fn value(&self, p: &Point) -> Float {
        self.primitive.value(&self.transform.t_point(*p))
    }
    fn normal(&self, p: &Point) -> Vector {
        self.transform
            .i_vector(self.primitive.normal(&self.transform.t_point(*p)))
            .normalize()
    }
}
impl<T: Primitive + 'static> Object for PrimitiveWrapper<T> {
    fn apply_transform(&mut self, other: &Transform) {
        self.transform = self.transform.concat(other)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpherePrimitive {
    radius: Float,
}

impl SpherePrimitive {
    pub fn new(r: Float) -> Box<SpherePrimitive> {
        Box::new(SpherePrimitive { radius: r })
    }
}

impl ImplicitFunction for SpherePrimitive {
    fn value(&self, p: &Point) -> Float {
        return p.to_vec().length() - self.radius;
    }
    fn normal(&self, p: &Point) -> Vector {
        return p.to_vec().normalize();
    }
}

impl Primitive for SpherePrimitive {}

pub type Sphere = PrimitiveWrapper<SpherePrimitive>;

impl Sphere {
    pub fn new(r: Float) -> Box<Sphere> {
        Box::new(Sphere {
            primitive: SpherePrimitive::new(r),
            transform: Transform::identity(),
        })
    }
}

pub trait Mixer: Clone + Debug {
    fn new(Float) -> Box<Self>;
    fn mixval(&self, a: Float, b: Float) -> Float;
    fn mixnormal(&self,
                 a: Float,
                 b: Float,
                 get_an: &Fn() -> Vector,
                 get_bn: &Fn() -> Vector)
                 -> Vector;
    fn r(&self) -> Float;
}

#[derive(Clone, Debug)]
pub struct Bool<T: Mixer> {
    a: Box<Object>,
    b: Box<Object>,
    mixer: Box<T>,
}

impl<T: Mixer + 'static> Bool<T> {
    pub fn new(a: Box<Object>, b: Box<Object>, r: Float) -> Box<Bool<T>> {
        Box::new(Bool::<T> {
            a: a,
            b: b,
            mixer: T::new(r),
        })
    }
    pub fn from_vec(mut v: Vec<Box<Object>>, r: Float) -> Option<Box<Object>> {
        match v.len() {
            0 => None,
            1 => Some(v.pop().unwrap()),
            _ => {
                let l2 = v.len() / 2;
                let v2 = v.split_off(l2);
                Some(Bool::<T>::new(Bool::<T>::from_vec(v, r).unwrap(),
                                    Bool::<T>::from_vec(v2, r).unwrap(),
                                    r))
            }
        }
    }
}


impl<T: Mixer + 'static> ImplicitFunction for Bool<T> {
    fn value(&self, p: &Point) -> Float {
        return self.mixer.mixval(self.a.value(p), self.b.value(p));
    }

    fn normal(&self, p: &Point) -> Vector {
        let va = self.a.value(p);
        let vb = self.b.value(p);
        if (va - vb).abs() < self.mixer.r() {
            normal_from_implicit(self, p)
        } else {
            self.mixer.mixnormal(va,
                                 vb,
                                 &|| self.a.normal(&p.clone()),
                                 &|| self.b.normal(&p.clone()))
        }
    }
}
impl<T: Mixer + 'static> Object for Bool<T> {
    fn apply_transform(&mut self, other: &Transform) {
        self.a.apply_transform(other);
        self.b.apply_transform(other);
    }
}

#[derive(Clone, Debug)]
pub struct UnionMixer {
    r: Float,
}

fn rmin(a: Float, b: Float, r: Float) -> Float {
    if (a - b).abs() < r {
        return b + r * (f64::consts::PI / 4. + ((a - b) / r / 2_f64.sqrt()).asin()).sin() - r;
    }
    a.min(b)
}

fn rmax(a: Float, b: Float, r: Float) -> Float {
    if (a - b).abs() < r {
        return b - r * (f64::consts::PI / 4. - ((a - b) / r / 2_f64.sqrt()).asin()).sin() + r;
    }
    a.max(b)
}

impl Mixer for UnionMixer {
    fn new(r: Float) -> Box<Self> {
        Box::new(UnionMixer { r: r })
    }
    fn mixval(&self, a: Float, b: Float) -> Float {
        rmin(a, b, self.r)
    }
    fn mixnormal(&self,
                 a: Float,
                 b: Float,
                 get_an: &Fn() -> Vector,
                 get_bn: &Fn() -> Vector)
                 -> Vector {
        if a < b {
            get_an()
        } else {
            get_bn()
        }
    }
    fn r(&self) -> Float {
        self.r
    }
}

pub type Union = Bool<UnionMixer>;

#[derive(Clone, Debug)]
pub struct IntersectionMixer {
    r: Float,
}
impl Mixer for IntersectionMixer {
    fn new(r: Float) -> Box<Self> {
        Box::new(IntersectionMixer { r: r })
    }
    fn mixval(&self, a: Float, b: Float) -> Float {
        rmax(a, b, self.r)
    }
    fn mixnormal(&self,
                 a: Float,
                 b: Float,
                 get_an: &Fn() -> Vector,
                 get_bn: &Fn() -> Vector)
                 -> Vector {
        if a > b {
            get_an()
        } else {
            get_bn()
        }
    }
    fn r(&self) -> Float {
        self.r
    }
}

pub type Intersection = Bool<IntersectionMixer>;

#[derive(Clone, Debug)]
pub struct SubtractionMixer {
    r: Float,
}
impl Mixer for SubtractionMixer {
    fn new(r: Float) -> Box<Self> {
        Box::new(SubtractionMixer { r: r })
    }
    fn mixval(&self, a: Float, b: Float) -> Float {
        rmax(a, -b, self.r)
    }
    fn mixnormal(&self,
                 a: Float,
                 b: Float,
                 get_an: &Fn() -> Vector,
                 get_bn: &Fn() -> Vector)
                 -> Vector {
        if a > -b {
            get_an()
        } else {
            get_bn() * -1.
        }
    }
    fn r(&self) -> Float {
        self.r
    }
}

pub type Subtraction = Bool<SubtractionMixer>;

impl Bool<SubtractionMixer> {
    pub fn subtraction_from_vec(mut v: Vec<Box<Object>>, r: Float) -> Option<Box<Object>> {
        match v.len() {
            0 => None,
            1 => Some(v.pop().unwrap()),
            _ => {
                let v_rest = v.split_off(1);
                Some(Subtraction::new(v.pop().unwrap(), Union::from_vec(v_rest, r).unwrap(), r))
            }
        }
    }
}

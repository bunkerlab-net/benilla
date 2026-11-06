// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, TransportMgr.cpp and Movement/spline.cpp.
// Upstream GPL-2.0-or-later; Rust translation GPL-3.0-or-later.
//! Source transport template numerics. Bits are IEEE RNE, with no contraction.
mod numeric;
use crate::facing::soft::{D, S};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    SourceUndefined(&'static str),
    NumericDomain,
    MissingMap(u32),
    CrossMapInstance,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source transport: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T, E = Error> = std::result::Result<T, E>;
fn finite(bits: u32) -> Result<S> {
    if bits & 0x7f80_0000 == 0x7f80_0000 {
        Err(Error::NumericDomain)
    } else {
        Ok(S::bits(bits))
    }
}
fn scalar(value: S) -> Result<u32> {
    finite(value.raw()).map(S::raw)
}
fn integer(value: u32) -> S {
    use rustc_apfloat::{ieee::Single, Float, Round};
    S::bits(
        Single::from_u128_r(u128::from(value), Round::NearestTiesToEven)
            .value
            .to_bits() as u32,
    )
}
fn lerp(a: [u32; 3], b: [u32; 3], t: S) -> Result<[u32; 3]> {
    let mut out = [0; 3];
    for axis in 0..3 {
        let a = finite(a[axis])?;
        out[axis] = scalar(a + (finite(b[axis])? - a) * t)?;
    }
    Ok(out)
}

/// Logical spline identity is its vector index in the enclosing template.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TransportSpline {
    pub points: Vec<[u32; 3]>,
    pub lengths: Vec<u64>,
    pub first: usize,
    pub last: usize,
}
impl TransportSpline {
    /// Noncyclic virtual controls, spline.cpp:241-272.
    pub fn from_controls(controls: &[[u32; 3]]) -> Result<Self> {
        Self::from_leg(controls, 0, controls.len())
    }
    fn from_leg(backing: &[[u32; 3]], start: usize, count: usize) -> Result<Self> {
        if count == 0 || start + count > backing.len() {
            return Err(Error::SourceUndefined("spline_controls"));
        }
        let next = *backing
            .get(start + 1)
            .ok_or(Error::SourceUndefined("spline_control_pair"))?;
        let mut points = Vec::with_capacity(count + 2);
        points.push(lerp(backing[start], next, S::bits(0xbf80_0000))?);
        points.extend_from_slice(&backing[start..start + count]);
        points.push(backing[start + count - 1]);
        Self::raw(points, 1, count)
    }
    fn raw(points: Vec<[u32; 3]>, first: usize, last: usize) -> Result<Self> {
        let mut spline = Self {
            points,
            lengths: vec![0; last + 1],
            first,
            last,
        };
        let mut length = D::ZERO;
        // Spline<double>::initLengths adds the rounded binary32 segment length to binary64.
        for i in first..last {
            let mut previous = spline.points[i];
            let mut segment = D::ZERO;
            for step in 1..=3 {
                let next = spline.evaluate(i, (integer(step) / integer(3)).raw())?;
                let delta =
                    std::array::from_fn::<_, 3, _>(|a| S::bits(next[a]) - S::bits(previous[a]));
                let square = (delta[0] * delta[0] + delta[1] * delta[1]) + delta[2] * delta[2];
                segment = segment + numeric::sqrt_single(square)?.double();
                previous = next;
            }
            length = length + segment.single().double();
            spline.lengths[i + 1] = length.raw();
        }
        Ok(spline)
    }
    fn evaluation(&self, index: usize, bits: u32, derivative: bool) -> Result<[u32; 3]> {
        if index < self.first || index >= self.last || index + 2 >= self.points.len() {
            return Err(Error::SourceUndefined("spline_index"));
        }
        let t = finite(bits)?;
        let one = S::bits(0x3f80_0000);
        let times = if derivative {
            [integer(3) * t * t, integer(2) * t, one, S::ZERO]
        } else {
            [t * t * t, t * t, t, one]
        };
        let coefficients = [
            [0xbf00_0000, 0x3fc0_0000, 0xbfc0_0000, 0x3f00_0000],
            [0x3f80_0000, 0xc020_0000, 0x4000_0000, 0xbf00_0000],
            [0xbf00_0000, 0, 0x3f00_0000, 0],
            [0, 0x3f80_0000, 0, 0],
        ];
        // Vector4.cpp:121-129 starts each dot product at +0 and adds all four terms.
        let mut weights = [S::ZERO; 4];
        for i in 0..4 {
            for j in 0..4 {
                weights[i] = weights[i] + times[j] * S::bits(coefficients[j][i]);
            }
        }
        let mut out = [0; 3];
        for (axis, output) in out.iter_mut().enumerate() {
            let p = |n: usize| finite(self.points[index - 1 + n][axis]);
            *output = scalar(
                ((p(0)? * weights[0] + p(1)? * weights[1]) + p(2)? * weights[2])
                    + p(3)? * weights[3],
            )?;
        }
        Ok(out)
    }
    /// Source segment t is deliberately not clamped, including outside [0,1].
    pub fn evaluate(&self, index: usize, t: u32) -> Result<[u32; 3]> {
        self.evaluation(index, t, false)
    }
    pub fn derivative(&self, index: usize, t: u32) -> Result<[u32; 3]> {
        self.evaluation(index, t, true)
    }
    pub fn length(&self, first: usize, last: usize) -> Result<u32> {
        let a = self
            .lengths
            .get(first)
            .ok_or(Error::SourceUndefined("spline_length_index"))?;
        let b = self
            .lengths
            .get(last)
            .ok_or(Error::SourceUndefined("spline_length_index"))?;
        scalar((D::bits(*b) - D::bits(*a)).single())
    }
}

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TaxiNode {
    pub id: u32,
    pub path: u32,
    pub index: u32,
    pub map: u32,
    pub position: [u32; 3],
    pub flags: u32,
    pub delay: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct MoTransport {
    pub entry: u32,
    pub path: u32,
    pub speed: u32,
    pub acceleration: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KeyFrame {
    pub node: usize,
    pub index: usize,
    pub orientation: u32,
    pub dist_since_stop: u32,
    pub dist_until_stop: u32,
    pub dist_from_prev: u32,
    pub time_from: u32,
    pub time_to: u32,
    pub teleport: bool,
    pub update: bool,
    pub arrive_time: u32,
    pub departure_time: u32,
    pub spline: Option<usize>,
    pub next_dist_from_prev: u32,
    pub next_arrive_time: u32,
}
impl KeyFrame {
    fn new(node: usize, orientation: u32) -> Self {
        Self {
            node,
            index: 0,
            orientation,
            dist_since_stop: 0xbf80_0000,
            dist_until_stop: 0xbf80_0000,
            dist_from_prev: 0xbf80_0000,
            time_from: 0,
            time_to: 0,
            teleport: false,
            update: false,
            arrive_time: 0,
            departure_time: 0,
            spline: None,
            next_dist_from_prev: 0,
            next_arrive_time: 0,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TransportTemplate {
    pub entry: u32,
    pub maps_used: BTreeSet<u32>,
    pub in_instance: bool,
    pub path_time: u32,
    pub accel_time: u32,
    pub accel_dist: u32,
    #[serde(serialize_with = "serialize_nodes")]
    pub nodes: Arc<[TaxiNode]>,
    pub key_frames: Vec<KeyFrame>,
    pub splines: Vec<TransportSpline>,
}
fn serialize_nodes<Sr: serde::Serializer>(
    nodes: &Arc<[TaxiNode]>,
    serializer: Sr,
) -> std::result::Result<Sr::Ok, Sr::Error> {
    nodes.as_ref().serialize(serializer)
}
impl TransportTemplate {
    /// Transports.cpp:89-90 changes only these fields, including a zero period.
    pub fn override_period(&mut self, period: u32) -> Result<()> {
        self.key_frames
            .last_mut()
            .ok_or(Error::SourceUndefined("empty_key_frames"))?
            .departure_time = period;
        self.path_time = period;
        Ok(())
    }
}

/// The source float atan2 result widens for +M_PI before binary32 normalization.
pub fn orientation_bits(y: u32, x: u32) -> Result<u32> {
    let a = crate::facing::atan2_bits(y, x).map_err(|_| Error::NumericDomain)?;
    let o = (S::bits(a).double() + D::bits(0x4009_21fb_5444_2d18)).single();
    let tau = S::bits(0x40c9_0fdb);
    scalar(if o < S::ZERO {
        -((-o).fmod(tau)) + tau
    } else {
        o.fmod(tau)
    })
}
fn milliseconds(time: S) -> Result<u32> {
    use rustc_apfloat::{ieee::Double, Float, Round, Status};
    let value = Double::from_bits(u128::from((time * integer(1000)).double().raw()));
    let integer = value.to_i128_r(64, Round::TowardZero, &mut false);
    if integer.status.contains(Status::INVALID_OP) {
        return Err(Error::SourceUndefined("float_to_uint32"));
    }
    u32::try_from(integer.value).map_err(|_| Error::SourceUndefined("float_to_uint32"))
}

/// GenerateWaypoints:76-302. Out-of-range path IDs are handled by the registry before this call.
pub fn generate_template(
    definition: MoTransport,
    nodes: Arc<[TaxiNode]>,
    maps: &BTreeMap<u32, u32>,
) -> Result<TransportTemplate> {
    if nodes.len() < 3 {
        return Err(Error::SourceUndefined(
            "orientation_controls_or_empty_frames",
        ));
    }
    let mut all_points = Vec::with_capacity(nodes.len() + 3);
    all_points.push(lerp(
        nodes[0].position,
        nodes[1].position,
        S::bits(0xbe4c_cccd),
    )?);
    all_points.extend(nodes.iter().map(|node| node.position));
    let n = all_points.len();
    all_points.push(lerp(
        all_points[n - 1],
        all_points[n - 2],
        S::bits(0xbe4c_cccd),
    )?);
    let n = all_points.len();
    all_points.push(lerp(
        all_points[n - 1],
        all_points[n - 2],
        S::bits(0xbf80_0000),
    )?);
    // Orientation lengths are unconsumed. Preserve its raw controls/index range, not an unused cache.
    let orientation = TransportSpline {
        first: 1,
        last: all_points.len() - 2,
        points: all_points,
        lengths: Vec::new(),
    };
    let mut frames: Vec<KeyFrame> = Vec::new();
    let mut points = Vec::new();
    let mut maps_used = BTreeSet::new();
    let mut skip = false;
    for i in 1..nodes.len() - 1 {
        if skip {
            skip = false;
            continue;
        }
        let node = nodes[i];
        if node.flags & 1 != 0 || node.map != nodes[i + 1].map {
            frames
                .last_mut()
                .ok_or(Error::SourceUndefined("teleport_without_previous_frame"))?
                .teleport = true;
            skip = true;
        } else {
            let h = orientation.derivative(i + 1, 0)?;
            frames.push(KeyFrame::new(i, orientation_bits(h[1], h[0])?));
            points.push(node.position);
            maps_used.insert(node.map);
        }
    }
    if frames.is_empty() {
        return Err(Error::SourceUndefined("empty_key_frames"));
    }
    let mut in_instance = false;
    for map in &maps_used {
        let kind = *maps.get(map).ok_or(Error::MissingMap(*map))?;
        let instanceable = matches!(kind, 1..=3);
        if maps_used.len() > 1 && instanceable {
            return Err(Error::CrossMapInstance);
        }
        in_instance = instanceable;
    }
    frames.last_mut().unwrap().teleport = true;
    let speed = integer(definition.speed);
    let accel = integer(definition.acceleration);
    let half = S::bits(0x3f00_0000);
    let two = integer(2);
    let accel_dist = half * speed * speed / accel;
    let accel_time = speed / accel;
    let stop = |frame: &KeyFrame| nodes[frame.node].flags == 2;
    let mut first_stop = if stop(&frames[0]) { Some(0) } else { None };
    let mut last_stop = first_stop;
    frames[0].dist_from_prev = 0;
    frames[0].index = 1;
    let mut start = 0;
    let mut splines = Vec::new();
    for i in 1..frames.len() {
        if frames[i - 1].teleport || i + 1 == frames.len() {
            let extra = usize::from(!frames[i - 1].teleport);
            let spline = TransportSpline::from_leg(&points, start, i - start + extra)?;
            let identity = splines.len();
            for j in start..i + extra {
                frames[j].index = j - start + 1;
                frames[j].dist_from_prev = spline.length(j - start, j + 1 - start)?;
                if j > 0 {
                    frames[j - 1].next_dist_from_prev = frames[j].dist_from_prev;
                }
                frames[j].spline = Some(identity);
            }
            if frames[i - 1].teleport {
                frames[i].index = i - start + 1;
                frames[i].dist_from_prev = 0;
                frames[i - 1].next_dist_from_prev = 0;
                frames[i].spline = Some(identity);
            }
            splines.push(spline);
            start = i;
        }
        if stop(&frames[i]) {
            first_stop.get_or_insert(i);
            last_stop = Some(i);
        }
    }
    frames.last_mut().unwrap().next_dist_from_prev = frames[0].dist_from_prev;
    let first_stop = first_stop.unwrap_or(0);
    let last_stop = last_stop.unwrap_or(0);
    let count = frames.len();
    let mut distance = S::ZERO;
    for i in 0..count {
        let j = (i + last_stop) % count;
        if stop(&frames[j]) || j == last_stop {
            distance = S::ZERO;
        } else {
            distance = distance + S::bits(frames[j].dist_from_prev);
        }
        frames[j].dist_since_stop = scalar(distance)?;
    }
    distance = S::ZERO;
    for i in (0..count).rev() {
        let j = (i + first_stop) % count;
        distance = distance + S::bits(frames[(j + 1) % count].dist_from_prev);
        frames[j].dist_until_stop = scalar(distance)?;
        if stop(&frames[j]) || j == first_stop {
            distance = S::ZERO;
        }
    }
    for frame in &mut frames {
        let since = S::bits(frame.dist_since_stop);
        let until = S::bits(frame.dist_until_stop);
        let total = since + until;
        let time = if total < two * accel_dist {
            if since < until {
                let segment = two * numeric::sqrt_single((until + since) / accel)?;
                segment - numeric::sqrt_single(two * since / accel)?
            } else {
                numeric::sqrt_single(two * until / accel)?
            }
        } else if since < accel_dist {
            let segment = (until + since) / speed + speed / accel;
            segment - numeric::sqrt_single(two * since / accel)?
        } else if until < accel_dist {
            numeric::sqrt_single(two * until / accel)?
        } else {
            until / speed + half * speed / accel
        };
        frame.time_to = scalar(time)?;
    }
    let mut segment = S::ZERO;
    for i in 0..count {
        let j = (i + last_stop) % count;
        if stop(&frames[j]) || j == last_stop {
            segment = S::bits(frames[j].time_to);
        }
        frames[j].time_from = scalar(segment - S::bits(frames[j].time_to))?;
    }
    let mut time = S::ZERO;
    if stop(&frames[0]) {
        time = integer(nodes[frames[0].node].delay);
        frames[0].departure_time = milliseconds(time)?;
    }
    for i in 1..count {
        time = time + S::bits(frames[i - 1].time_to);
        if stop(&frames[i]) {
            frames[i].arrive_time = milliseconds(time)?;
            frames[i - 1].next_arrive_time = frames[i].arrive_time;
            time = time + integer(nodes[frames[i].node].delay);
            frames[i].departure_time = milliseconds(time)?;
        } else {
            time = time - S::bits(frames[i].time_to);
            frames[i].arrive_time = milliseconds(time)?;
            frames[i - 1].next_arrive_time = frames[i].arrive_time;
            frames[i].departure_time = frames[i].arrive_time;
        }
    }
    frames.last_mut().unwrap().next_arrive_time = frames.last().unwrap().departure_time;
    if matches!(definition.path, 293 | 303) {
        frames
            .get_mut(12)
            .ok_or(Error::SourceUndefined("refresh_frame_12"))?
            .update = true;
    }
    Ok(TransportTemplate {
        entry: definition.entry,
        maps_used,
        in_instance,
        path_time: frames.last().unwrap().departure_time,
        accel_time: scalar(accel_time)?,
        accel_dist: scalar(accel_dist)?,
        nodes,
        key_frames: frames,
        splines,
    })
}

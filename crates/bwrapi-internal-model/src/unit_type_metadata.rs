//! BWAPI UnitType tables by unit type id: transport space provided and the
//! default maximum energy (raw, 1/256 energy).

pub(crate) const UNIT_TYPE_COUNT: usize = 234;

const fn provided() -> [u8; UNIT_TYPE_COUNT] { let mut t = [0; UNIT_TYPE_COUNT]; t[11]=8;t[42]=8;t[57]=8;t[69]=8;t[125]=4;t }
const fn max_energy() -> [i32; UNIT_TYPE_COUNT] { let mut t = [0; UNIT_TYPE_COUNT]; t[1]=51200;t[8]=51200;t[9]=51200;t[12]=51200;t[16]=64000;t[21]=64000;t[22]=64000;t[27]=64000;t[28]=64000;t[29]=64000;t[34]=51200;t[45]=51200;t[46]=51200;t[49]=64000;t[51]=64000;t[52]=64000;t[60]=51200;t[63]=51200;t[67]=51200;t[71]=51200;t[79]=64000;t[86]=64000;t[87]=64000;t[98]=64000;t[99]=64000;t[100]=64000;t[102]=64000;t[104]=64000;t[107]=51200;t[172]=51200;t }
const SPACE_PROVIDED: [u8; UNIT_TYPE_COUNT] = provided();
const MAX_ENERGY_RAW: [i32; UNIT_TYPE_COUNT] = max_energy();
pub fn cargo_space_provided(id: u16) -> Option<u8> { SPACE_PROVIDED.get(id as usize).copied() }
pub fn max_energy_raw(id: u16) -> Option<i32> { MAX_ENERGY_RAW.get(id as usize).copied() }

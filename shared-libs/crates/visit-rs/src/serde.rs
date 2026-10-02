use serde::{Serialize, Serializer};

use crate::{Visit, Visitor};

impl<Ser, Ok, Err> Visitor for Ser
where
    for<'x> &'x mut Ser: Serializer<Ok = Ok, Error = Err>,
{
    type Result = Result<Ok, Err>;
}

impl<Ser, Ok, Err, T> Visit<Ser> for T
where
    T: Serialize,
    for<'x> &'x mut Ser: Serializer<Ok = Ok, Error = Err>,
{
    fn visit(&self, serializer: &mut Ser) -> <Ser as Visitor>::Result {
        self.serialize(serializer)
    }
}

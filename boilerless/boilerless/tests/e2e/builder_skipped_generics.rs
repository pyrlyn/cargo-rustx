//! Skipped fields may be the only use of a builder's type or lifetime parameter.
use boilerless::Boil;
use std::marker::PhantomData;

#[derive(Boil)]
#[boil(builder)]
struct Value<T> where T: Default {
    #[boil(skip)]
    value: T,
}

#[derive(Boil)]
#[boil(builder)]
struct Borrowed<'a, T: 'a, const N: usize> {
    #[boil(skip)]
    lifetime: PhantomData<&'a T>,
    #[boil(skip, default = [0; N])]
    data: [u8; N],
    __boil_marker: u8,
}

fn main() {
    fn send<T: Send>(_: T) {}
    send(Value::<std::rc::Rc<()>>::builder());
    let value: Value<String> = Value::builder().build().unwrap();
    assert_eq!(value.value, "");
    let borrowed = Borrowed::<'_, String, 3>::builder().__boil_marker(9u8).build().unwrap();
    assert_eq!(borrowed.data, [0; 3]);
    assert_eq!(borrowed.__boil_marker, 9);
}

use std::iter::Peekable;

pub struct TakeWhilePeek<'a, I, P> {
    iter: &'a mut I,
    flag: bool,
    predicate: P,
}

// the following code is slightly adapted from `std::iter::adapters::TakeWhile
impl<'a, I: Iterator, P: for<'b> FnMut(&'b I::Item) -> bool> Iterator
    for TakeWhilePeek<'a, Peekable<I>, P>
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.flag {
            None
        } else {
            if (self.predicate)(self.iter.peek()?) {
                self.iter.next()
            } else {
                self.flag = true;
                None
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.flag {
            (0, Some(0))
        } else {
            let (_, upper) = self.iter.size_hint();
            (0, upper) // can't know a lower bound, due to the predicate
        }
    }
}

pub fn eat_while<I: Iterator, F: FnMut(&I::Item) -> bool>(
    iter: &mut Peekable<I>,
    check: F,
) -> TakeWhilePeek<Peekable<I>, F> {
    TakeWhilePeek {
        iter,
        flag: false,
        predicate: check,
    }
}

pub fn eat_while_lvl_gt0<I: Iterator>(
    iter: &mut Peekable<I>,
    mut inc_lvl: impl FnMut(&I::Item) -> bool + 'static,
    mut dec_lvl: impl FnMut(&I::Item) -> bool + 'static,
) -> TakeWhilePeek<Peekable<I>, Box<dyn FnMut(&I::Item) -> bool>>
where
    I::Item: PartialEq,
{
    let mut lvl = 0;
    eat_while(
        iter,
        Box::new(move |x| {
            if inc_lvl(x) {
                lvl += 1;
            } else if dec_lvl(x) {
                if lvl > 0 {
                    lvl -= 1;
                } else {
                    return false;
                }
            }
            true
        }),
    )
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Containerized<T> {
    Free(T),
    Contained(Vec<Self>),
}

impl<T> Containerized<T> {
    fn map_inner<U, F: FnMut(T) -> U>(self, f: &mut F) -> Containerized<U> {
        match self {
            Containerized::Free(t) => Containerized::Free(f(t)),
            Containerized::Contained(v) => {
                Containerized::Contained(v.into_iter().map(|c| c.map_inner(&mut *f)).collect())
            }
        }
    }

    pub fn map<U, F: FnMut(T) -> U>(self, mut f: F) -> Containerized<U> {
        self.map_inner(&mut f)
    }

    fn flat_map_inner<I: IntoIterator, F: FnMut(T) -> I>(
        self,
        f: &mut F,
    ) -> Vec<Containerized<I::Item>> {
        match self {
            Containerized::Free(t) => f(t).into_iter().map(Containerized::Free).collect(),
            Containerized::Contained(v) => vec![Containerized::Contained(
                v.into_iter()
                    .flat_map(|c| c.flat_map_inner(&mut *f))
                    .collect(),
            )],
        }
    }

    pub fn flat_map<I: IntoIterator, F: FnMut(T) -> I>(
        self,
        mut f: F,
    ) -> Vec<Containerized<I::Item>> {
        self.flat_map_inner(&mut f)
    }

    pub fn join<U: Into<T> + Clone>(self, left: U, right: U) -> Vec<T> {
        match self {
            Containerized::Free(t) => vec![t],
            Containerized::Contained(v) => {
                let mut res = vec![left.clone().into()];
                res.append(
                    &mut v
                        .into_iter()
                        .flat_map(|c| c.join(left.clone(), right.clone()))
                        .collect(),
                );
                res.push(right.clone().into());
                res
            }
        }
    }
}

pub fn containerize<I: Iterator>(
    iter: &mut Peekable<I>,
    mut left: impl FnMut(&I::Item) -> bool,
    mut right: impl FnMut(&I::Item) -> bool,
) -> Vec<Containerized<Vec<I::Item>>> {
    let mut stack: Vec<Vec<Containerized<Vec<I::Item>>>> = vec![vec![]];

    while let Some(t) = iter.next() {
        if left(&t) {
            stack.push(Vec::new());
        } else if right(&t) {
            // todo: proper error handling
            let v = stack.pop().unwrap();
            stack
                .last_mut()
                .expect("Unmatched right delimeter")
                .push(Containerized::Contained(v));
        } else {
            let last = stack.last_mut().unwrap();
            if let Some(Containerized::Free(v)) = last.last_mut() {
                v.push(t);
            } else {
                last.push(Containerized::Free(vec![t]));
            }
        }
    }

    if stack.len() > 1 {
        panic!("Unmatched left delimeter");
    }

    stack.pop().unwrap()
}

pub struct AutoEscape<I, F> {
    iter: I,
    is_esc: F,
}

impl<I: Iterator, F: FnMut(&I::Item) -> bool> Iterator for AutoEscape<I, F> {
    type Item = (bool, I::Item);

    fn next(&mut self) -> Option<Self::Item> {
        let nx = self.iter.next()?;
        if (self.is_esc)(&nx) {
            match self.iter.next() {
                Some(t) => Some((true, t)),
                None => Some((false, nx)),
            }
        } else {
            Some((false, nx))
        }
    }
}

pub trait CreateAutoEscape: Sized + Iterator {
    fn auto_escape<F: FnMut(&Self::Item) -> bool>(self, is_esc: F) -> AutoEscape<Self, F>;
}

impl<I: Iterator> CreateAutoEscape for I {
    fn auto_escape<F: FnMut(&Self::Item) -> bool>(self, is_esc: F) -> AutoEscape<Self, F> {
        AutoEscape { iter: self, is_esc }
    }
}

pub struct SplitIter<I: Iterator, F> {
    curr_len: usize,
    max_len: Option<usize>,
    iter: Peekable<I>,
    is_sep: Option<F>,
    keep_sep: bool,
    handle_sep: bool,
}

impl<I: Iterator, F: FnMut(&I::Item) -> bool> Iterator for SplitIter<I, F> {
    type Item = Vec<I::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        let _ = self.iter.peek()?;
        if self.iter.size_hint() == (0, Some(0)) {
            return None;
        }
        if self.handle_sep {
            // return the separator as its own item
            let sep = self.iter.next()?;
            self.handle_sep = false;
            return Some(vec![sep]);
        }
        if self.max_len.map_or(false, |len| self.curr_len == len) {
            return Some(self.iter.by_ref().collect());
        }
        self.curr_len += 1;
        let mut is_sep = self.is_sep.take().unwrap();
        let res = if self.keep_sep {
            // this keeps the separator in the iter
            self.handle_sep = true;
            eat_while(&mut self.iter, |t| !is_sep(t)).collect()
        } else {
            // this practically voids the separator
            self.iter.by_ref().take_while(|t| !is_sep(t)).collect()
        };
        self.is_sep = Some(is_sep);
        Some(res)
    }
}

pub trait IterSplit: Sized + IntoIterator {
    fn split_impl<F: FnMut(&Self::Item) -> bool>(
        self,
        max_len: Option<usize>,
        is_sep: F,
        keep_sep: bool,
    ) -> SplitIter<Self::IntoIter, F>;

    fn split<F: FnMut(&Self::Item) -> bool>(
        self,
        is_sep: F,
        keep_sep: bool,
    ) -> SplitIter<Self::IntoIter, F> {
        self.split_impl(None, is_sep, keep_sep)
    }

    fn splitn<F: FnMut(&Self::Item) -> bool>(
        self,
        n: usize,
        is_sep: F,
        keep_sep: bool,
    ) -> SplitIter<Self::IntoIter, F> {
        self.split_impl(Some(n), is_sep, keep_sep)
    }
}

impl<I: IntoIterator> IterSplit for I {
    fn split_impl<F: FnMut(&Self::Item) -> bool>(
        self,
        max_len: Option<usize>,
        is_sep: F,
        keep_sep: bool,
    ) -> SplitIter<Self::IntoIter, F> {
        SplitIter {
            curr_len: 1,
            max_len,
            iter: self.into_iter().peekable(),
            is_sep: Some(is_sep),
            keep_sep,
            handle_sep: false,
        }
    }
}

pub fn str_split_keep_sep<'a, F: FnMut(&char) -> bool + 'a>(
    s: &'a str,
    is_sep: F,
) -> impl Iterator<Item = String> + 'a {
    s.chars()
        .split(is_sep, true)
        .map(|v| v.into_iter().collect())
}

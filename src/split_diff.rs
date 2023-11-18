use itertools::Itertools;

const MAX_SAME_BEFORE_COLLAPSE: usize = 15;

#[derive(Debug)]
pub enum SplitDiffCell<T> {
    Hidden,
    Collapsed,

    Default(T),
    Insert(T),
    Delete(T),
}

pub fn build_split_diff<T>(
    old: &[T],
    new: &[T],
    diff_ops: &[similar::DiffOp],
) -> Vec<(SplitDiffCell<T>, SplitDiffCell<T>)>
where
    T: Clone,
{
    let mut cells: Vec<(SplitDiffCell<T>, SplitDiffCell<T>)> = vec![];

    for op in diff_ops {
        match *op {
            similar::DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                let mut old = old[old_index..old_index + len].to_vec();
                let mut new = new[new_index..new_index + len].to_vec();

                if len >= MAX_SAME_BEFORE_COLLAPSE * 2 {
                    old.drain(MAX_SAME_BEFORE_COLLAPSE..(len - MAX_SAME_BEFORE_COLLAPSE));
                    new.drain(MAX_SAME_BEFORE_COLLAPSE..(len - MAX_SAME_BEFORE_COLLAPSE));
                }

                for (i, pair) in old.iter().zip_longest(new.iter()).enumerate() {
                    if (i == MAX_SAME_BEFORE_COLLAPSE) && (len >= MAX_SAME_BEFORE_COLLAPSE * 2) {
                        cells.push((SplitDiffCell::Collapsed, SplitDiffCell::Collapsed));
                    }

                    match pair {
                        itertools::EitherOrBoth::Both(old, new) => cells.push((
                            SplitDiffCell::Default(old.clone()),
                            SplitDiffCell::Default(new.clone()),
                        )),
                        itertools::EitherOrBoth::Left(old) => {
                            cells.push((SplitDiffCell::Default(old.clone()), SplitDiffCell::Hidden))
                        }
                        itertools::EitherOrBoth::Right(new) => {
                            cells.push((SplitDiffCell::Hidden, SplitDiffCell::Default(new.clone())))
                        }
                    }
                }
            }
            similar::DiffOp::Delete {
                old_index,
                old_len,
                new_index: _,
            } => {
                for old_item in old[old_index..old_index + old_len].iter() {
                    cells.push((
                        SplitDiffCell::Delete(old_item.clone()),
                        SplitDiffCell::Hidden,
                    ))
                }
            }
            similar::DiffOp::Insert {
                old_index: _,
                new_index,
                new_len,
            } => {
                for new_item in new[new_index..new_index + new_len].iter() {
                    cells.push((
                        SplitDiffCell::Hidden,
                        SplitDiffCell::Insert(new_item.clone()),
                    ))
                }
            }
            similar::DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                for pair in old[old_index..old_index + old_len]
                    .iter()
                    .zip_longest(new[new_index..new_index + new_len].iter())
                {
                    match pair {
                        itertools::EitherOrBoth::Both(old, new) => cells.push((
                            SplitDiffCell::Delete(old.clone()),
                            SplitDiffCell::Insert(new.clone()),
                        )),
                        itertools::EitherOrBoth::Left(old) => {
                            cells.push((SplitDiffCell::Delete(old.clone()), SplitDiffCell::Hidden))
                        }
                        itertools::EitherOrBoth::Right(new) => {
                            cells.push((SplitDiffCell::Hidden, SplitDiffCell::Insert(new.clone())))
                        }
                    }
                }
            }
        }
    }

    cells
}

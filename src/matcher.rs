use std::collections::HashMap;

use crate::models::Product;

#[derive(Debug)]
pub struct OrderCount(pub u32);

#[derive(Debug)]
pub struct Book {
    pub buys: OrderCount,
    pub sells: OrderCount,
}

#[derive(Debug)]
pub struct Matcher {
    pub books: HashMap<Product, Book>,
}

#[derive(Debug)]
pub struct Match {
    pub product: Product,
}

impl Matcher {
    pub fn new() -> Self {
        Self {
            books: HashMap::new(),
        }
    }

    fn get_book(&mut self, product: Product) -> &mut Book {
        self.books.entry(product).or_insert_with(|| Book {
            buys: OrderCount(0),
            sells: OrderCount(0),
        })
    }

    pub fn add_buy(&mut self, product: Product) -> Option<Match> {
        let book = self.get_book(product);
        if book.sells.0 > 0 {
            book.sells.0 -= 1;
            Some(Match { product })
        } else {
            book.buys.0 += 1;
            None
        }
    }

    pub fn add_sell(&mut self, product: Product) -> Option<Match> {
        let book = self.get_book(product);
        if book.buys.0 > 0 {
            book.buys.0 -= 1;
            Some(Match { product })
        } else {
            book.sells.0 += 1;
            None
        }
    }
}

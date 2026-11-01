#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub declarations: Vec<Declaration>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub kind: DeclarationKind,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeclarationKind {
    Function,
    Api,
    Use,
}

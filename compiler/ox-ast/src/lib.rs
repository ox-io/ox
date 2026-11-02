#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub declarations: Vec<Declaration>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub kind: DeclarationKind,
    pub name: String,
    pub fields: Vec<Field>,
    pub variants: Vec<Variant>,
    pub params: Vec<Parameter>,
    pub return_type: Option<String>,
    pub body: Option<Block>,
    pub routes: Vec<Route>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub type_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub method: String,
    pub path: String,
    pub params: Vec<Parameter>,
    pub return_type: String,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub tail: Option<Box<Expression>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    Let {
        name: String,
        mutable: bool,
        type_name: Option<String>,
        value: Expression,
    },
    Assign {
        name: String,
        value: Expression,
    },
    Loop {
        condition: Expression,
        body: Block,
    },
    Break,
    Continue,
    Return(Expression),
    Fail(Expression),
    Expression(Expression),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expression {
    Integer(i64),
    String(String),
    Boolean(bool),
    Identifier(String),
    Object(Vec<(String, Expression)>),
    Construct {
        name: String,
        fields: Vec<(String, Expression)>,
    },
    Member {
        object: Box<Expression>,
        field: String,
    },
    Call {
        callee: Box<Expression>,
        args: Vec<Expression>,
    },
    Unary {
        operator: String,
        value: Box<Expression>,
    },
    Binary {
        left: Box<Expression>,
        operator: String,
        right: Box<Expression>,
    },
    If {
        condition: Box<Expression>,
        then_branch: Block,
        else_branch: Block,
    },
    Pick {
        value: Box<Expression>,
        cases: Vec<PickCase>,
        other: Option<Block>,
    },
    Try {
        value: Box<Expression>,
        binding: String,
        handler: Block,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickCase {
    pub variant: String,
    pub binding: Option<String>,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub type_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub name: String,
    pub payload_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeclarationKind {
    Function,
    Api,
    Use,
    Shape,
    Choice,
}

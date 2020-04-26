use baml::backend_html::BackendHtml;
use baml::{parse, Backend};

fn main() {
    let s = include_str!("test.baml").to_string();
    let ast = parse(s);
    // println!("{:#?}", ast);
    let s = BackendHtml::from_template_file("/Users/tomjonas/blog/index_template.html")
        .expect("IO error")
        .compile_ast(ast.expect("Err"));
    println!("=== done\n{}", s);
}

fn main() {
    let script = std::env::args().nth(1).expect("script");
    let g = d2_little::compiler::compile("", &script).expect("compile");

    println!("objects:");
    for (i, obj) in g.objects.iter().enumerate() {
        println!(
            "  [{i}] id={} abs={} parent={:?} near={:?} shape={} kids={}",
            obj.id,
            obj.abs_id(),
            obj.parent,
            obj.near_key,
            obj.shape.value,
            obj.children_array.len()
        );
    }

    println!("edges:");
    for (i, edge) in g.edges.iter().enumerate() {
        println!(
            "  [{i}] {} src={} dst={} scope={:?}",
            edge.abs_id(),
            g.objects[edge.src].abs_id(),
            g.objects[edge.dst].abs_id(),
            edge.scope_obj
        );
    }
}

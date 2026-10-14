// The Photos list on its own: the entry of the scroll benchmarks
// (`yf trace examples/gallery/Bench.tsx --scroll 3000`, the `list_bench`
// test).
import { render, root } from "yf";
import { Photos } from "./demos/Photos";

render(() => <Photos />, root);

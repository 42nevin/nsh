// The Rust gallery: the app is one native component; everything it shows
// is written in Rust (`native/`, the app's crate: nsh.json "rust").
import { mount } from "nsh";
import { Gallery } from "gallery/native";

mount(() => <Gallery />);

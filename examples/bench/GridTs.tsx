// `nsh trace examples/bench/GridTs.tsx`: the Grid screen with the Ts Button.
import { mount } from "nsh";
import { Button } from "./TsButton";
import { Grid } from "./buttons";

mount(() => <Grid Button={Button} />);

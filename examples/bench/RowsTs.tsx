// `nsh trace examples/bench/RowsTs.tsx`: the Rows screen with the Ts Button.
import { mount } from "nsh";
import { Button } from "./TsButton";
import { Rows } from "./buttons";

mount(() => <Rows Button={Button} />);

import { signal } from "nsh";
import logo from "../logo@2x.png";

const photo = "https://picsum.photos/seed/nsh-tailwind/600/400";

function Section(props: { title: string; children: any }) {
  return (
    <view class="gap-2">
      <text class="text-xs font-semibold uppercase tracking-wide text-zinc-500 dark:text-zinc-400">{props.title}</text>
      <view class="gap-3 rounded-card bg-white p-4 dark:bg-zinc-800">{props.children}</view>
    </view>
  );
}

function Label(props: { children: string }) {
  return <text class="text-center text-[11px] text-zinc-500 dark:text-zinc-400">{props.children}</text>;
}

/** Buttons: press states without JS, transitions, `disabled:`. */
function Buttons() {
  const count = signal(0);
  const busy = signal(false);
  return (
    <view class="gap-3">
      <view class="flex-row flex-wrap gap-2">
        <view
          onPress={() => count.set(count() + 1)}
          class="rounded-lg bg-brand-600 px-4 py-2 transition-colors duration-150 active:bg-brand-700 hover:bg-brand-500"
        >
          <text class="font-medium text-white">Pressed {count()}×</text>
        </view>
        <view
          onPress={() => count.set(0)}
          class="rounded-lg border border-zinc-300 px-4 py-2 transition active:scale-95 active:bg-zinc-100 dark:border-zinc-600 dark:active:bg-zinc-700"
        >
          <text class="font-medium text-zinc-900 dark:text-white">Reset</text>
        </view>
        <view
          onPress={() => busy.set(!busy())}
          disabled={busy()}
          class="rounded-full bg-emerald-500 px-4 py-2 disabled:opacity-40 active:bg-emerald-600"
        >
          <text class="font-medium text-white">{busy() ? "Disabled" : "Enabled"}</text>
        </view>
      </view>
      <view onPress={() => busy.set(false)} class="self-start rounded-md px-2 py-1 active:bg-zinc-100 dark:active:bg-zinc-700">
        <text class="text-sm text-brand-600 underline decoration-dotted underline-offset-4">Enable again</text>
      </view>
    </view>
  );
}

function Cards() {
  return (
    <view class="gap-3">
      <view class="group flex-row items-center gap-3 rounded-2xl bg-white p-3 shadow-lg ring-1 ring-black/5 transition-transform duration-200 active:scale-[0.98] dark:bg-zinc-700">
        <image src={logo} class="size-12 rounded-xl" />
        <view class="flex-1 gap-0.5">
          <text class="text-base font-semibold text-zinc-900 dark:text-white">Group press</text>
          <text class="text-sm text-zinc-500 group-active:text-brand-600 dark:text-zinc-300">Press the card: this line changes color</text>
        </view>
        <text class="text-2xl text-zinc-300 transition-transform group-active:translate-x-1">›</text>
      </view>
      <view class="overflow-hidden rounded-2xl bg-linear-to-br from-brand-500 via-fuchsia-500 to-orange-400 p-5 shadow-xl shadow-brand-500/40">
        <text class="text-xl font-bold text-white">Gradient card</text>
        <text class="mt-1 text-sm text-white/80">bg-linear-to-br from-brand-500 via-fuchsia-500 to-orange-400</text>
      </view>
    </view>
  );
}

function Layout() {
  return (
    <view class="gap-3">
      <view class="flex-row gap-2">
        <view class="h-10 flex-1 rounded-md bg-sky-200" />
        <view class="h-10 flex-[2] rounded-md bg-sky-400" />
        <view class="h-10 w-10 rounded-md bg-sky-600" />
      </view>
      <view class="grid grid-cols-3 gap-2">
        <view class="col-span-2 h-12 items-center justify-center rounded-md bg-violet-200">
          <text class="text-xs text-violet-900">col-span-2</text>
        </view>
        <view class="h-12 rounded-md bg-violet-300" />
        <view class="h-12 rounded-md bg-violet-400" />
        <view class="row-span-2 rounded-md bg-violet-500" />
        <view class="h-12 rounded-md bg-violet-600" />
        <view class="h-12 rounded-md bg-violet-700" />
        <view class="h-12 rounded-md bg-violet-800" />
      </view>
      <view class="flex-col gap-2 sm:flex-row">
        <view class="h-8 flex-1 rounded-md bg-amber-300" />
        <view class="h-8 flex-1 rounded-md bg-amber-500" />
      </view>
      <Label>The last row is a column on phones and a row from `sm:` (640pt) up.</Label>
    </view>
  );
}

const palette = ["red", "orange", "amber", "emerald", "sky", "indigo", "pink", "zinc"];
const shades = ["100", "300", "500", "700", "900"];

function Colors() {
  return (
    <view class="gap-1">
      <each of={palette}>
        {(name) => (
          <view class="flex-row gap-1">
            <text class="w-16 text-xs text-zinc-500">{name()}</text>
            <each of={shades}>{(shade) => <view class={`h-6 flex-1 rounded bg-${name()}-${shade()}`} />}</each>
          </view>
        )}
      </each>
    </view>
  );
}

function Shadows() {
  const sizes = ["shadow-xs", "shadow-sm", "shadow-md", "shadow-lg", "shadow-xl", "shadow-2xl"];
  return (
    <view class="gap-4">
      <view class="flex-row flex-wrap justify-between gap-y-5 px-1 py-2">
        <each of={sizes}>
          {(s) => (
            <view class="gap-1">
              <view class={`size-14 rounded-xl bg-white dark:bg-zinc-600 ${s()}`} />
              <Label>{s().slice(7)}</Label>
            </view>
          )}
        </each>
      </view>
      <view class="flex-row justify-between px-1">
        <view class="gap-1">
          <view class="size-14 rounded-xl bg-zinc-100 inset-shadow-sm" />
          <Label>inset</Label>
        </view>
        <view class="gap-1">
          <view class="size-14 rounded-xl bg-white ring-2 ring-sky-500 ring-offset-2" />
          <Label>ring</Label>
        </view>
        <view class="gap-1">
          <view class="size-14 rounded-full bg-white outline-2 outline-offset-2 outline-dashed outline-pink-500" />
          <Label>outline</Label>
        </view>
        <view class="gap-1">
          <view class="size-14 rounded-xl bg-emerald-500 shadow-lg shadow-emerald-500/50" />
          <Label>colored</Label>
        </view>
      </view>
    </view>
  );
}

function Gradients() {
  const kinds = [
    "bg-linear-to-r from-sky-400 to-blue-600",
    "bg-linear-45 from-rose-400 via-fuchsia-500 to-indigo-500",
    "bg-radial from-yellow-200 to-orange-500",
    "bg-conic from-red-500 via-green-500 to-red-500",
  ];
  return (
    <view class="flex-row justify-between">
      <each of={kinds}>{(k) => <view class={`size-16 rounded-xl ${k()}`} />}</each>
    </view>
  );
}

function Typography() {
  return (
    <view class="gap-2">
      <text class="text-3xl font-black tracking-tight text-zinc-900 dark:text-white">Display</text>
      <text class="text-lg font-light text-zinc-700 dark:text-zinc-200">Light, large and relaxed</text>
      <text class="font-serif text-lg italic text-zinc-700 dark:text-zinc-200">Serif italic</text>
      <text class="font-mono text-sm text-zinc-700 tabular-nums dark:text-zinc-200">font-mono 0123456789</text>
      <view class="flex-row flex-wrap gap-x-4 gap-y-1 text-zinc-800 dark:text-zinc-100">
        <text class="underline decoration-wavy decoration-red-500">wavy</text>
        <text class="underline decoration-double decoration-sky-500">double</text>
        <text class="underline decoration-dashed decoration-2">dashed</text>
        <text class="line-through decoration-zinc-400">struck</text>
        <text class="uppercase tracking-widest text-xs font-semibold text-brand-600">caps</text>
      </view>
      <text class="truncate text-sm text-zinc-500">truncate: a single line that is far too long to fit in the card cut with an ellipsis</text>
      <text class="line-clamp-2 text-sm leading-relaxed text-zinc-500">
        line-clamp-2: Text is laid out once on the engine thread and drawn on the main thread; classes reach it through the same style path as everything
        else, so the third line of this paragraph never shows.
      </text>
      <text class="text-shadow-md text-xl font-bold text-white bg-sky-500 rounded-md px-2 self-start">text-shadow-md</text>
    </view>
  );
}

function Effects() {
  return (
    <view class="gap-3">
      <view class="flex-row justify-between">
        <each of={["", "grayscale", "sepia", "blur-sm", "brightness-50"]}>
          {(f) => (
            <view class="gap-1">
              <image src={photo} class={`size-14 rounded-lg ${f()}`} />
              <Label>{f() || "none"}</Label>
            </view>
          )}
        </each>
      </view>
      <view class="h-28 overflow-hidden rounded-xl">
        <image src={photo} class="absolute inset-0 size-full" />
        <view class="absolute inset-x-3 bottom-3 rounded-lg bg-white/40 px-3 py-2 backdrop-blur-md">
          <text class="text-sm font-semibold text-zinc-900">backdrop-blur-md bg-white/40</text>
        </view>
        <view class="absolute right-3 top-3 size-10 rounded-full bg-yellow-300 mix-blend-multiply" />
      </view>
    </view>
  );
}

function Motion() {
  return (
    <view class="flex-row items-center justify-between px-2 py-3">
      <view class="gap-2 items-center">
        <view class="size-8 rounded-md bg-brand-500 animate-spin" />
        <Label>spin</Label>
      </view>
      <view class="gap-2 items-center">
        <view class="size-8 items-center justify-center">
          <view class="absolute size-4 rounded-full bg-sky-400 animate-ping" />
          <view class="size-4 rounded-full bg-sky-500" />
        </view>
        <Label>ping</Label>
      </view>
      <view class="gap-2 items-center">
        <view class="h-8 w-14 rounded-md bg-zinc-300 animate-pulse" />
        <Label>pulse</Label>
      </view>
      <view class="gap-2 items-center">
        <view class="size-8 rounded-full bg-orange-400 animate-bounce" />
        <Label>bounce</Label>
      </view>
      <view class="gap-2 items-center">
        <view class="size-8 rotate-12 skew-x-6 scale-90 rounded-md bg-emerald-500" />
        <Label>transform</Label>
      </view>
    </view>
  );
}

function Lists() {
  const items = ["Inbox", "Drafts", "Sent", "Archive", "Trash"];
  return (
    <view class="overflow-hidden rounded-xl border border-zinc-200 dark:border-zinc-700">
      <each of={items}>
        {(item) => (
          <view class="flex-row items-center justify-between border-t border-zinc-200 px-3 py-2.5 first:border-t-0 even:bg-zinc-50 last:bg-rose-50 active:bg-brand-50 dark:border-zinc-700 dark:even:bg-zinc-900 dark:last:bg-rose-950">
            <text class="text-zinc-800 dark:text-zinc-100">{item()}</text>
            <text class="text-xs text-zinc-400">›</text>
          </view>
        )}
      </each>
    </view>
  );
}

function Inputs() {
  return (
    <view class="gap-2">
      <input
        placeholder="Focus me"
        class="rounded-lg border border-zinc-300 px-3 py-2 text-base text-zinc-900 placeholder:text-zinc-400 caret-brand-600 focus:border-brand-500 focus:ring-2 focus:ring-brand-500/30 dark:border-zinc-600 dark:text-white"
      />
      <Label>focus: border and ring, placeholder: color, caret color</Label>
    </view>
  );
}

const sections: [string, () => any][] = [
  ["Buttons: active, hover, transition, disabled", Buttons],
  ["Cards: shadow, ring, group, gradient", Cards],
  ["Flex, grid, breakpoints", Layout],
  ["Palette", Colors],
  ["Shadows, rings, outlines", Shadows],
  ["Gradients", Gradients],
  ["Typography", Typography],
  ["Filters, backdrop blur, blend modes", Effects],
  ["Animations and transforms", Motion],
  ["first:, even:, last:", Lists],
  ["Inputs", Inputs],
];

export function Tailwind() {
  return (
    <scroll class="flex-1 bg-zinc-100 dark:bg-zinc-900">
      <view class="gap-5 p-4">
        <each of={sections}>
          {(section) => {
            const [title, Body] = section();
            return (
              <Section title={title}>
                <Body />
              </Section>
            );
          }}
        </each>
      </view>
    </scroll>
  );
}

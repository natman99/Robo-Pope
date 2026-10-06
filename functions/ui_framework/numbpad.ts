import { ButtonStyle } from "discord.js";
import client from "../..";
import { Button, Page, TextDisplay, ActionRow } from "./ui";

const button1 = new Button(
  async (p, i, d: number) => {await p.update(d, i)},
  { style: ButtonStyle.Secondary, label: "1" },
  async (_p: any, index: number, _i: any) => {return (index ?? 0) * 10 + 1}
)
const button2 = new Button(
  async (p, i, d: number) => {await p.update(d, i)},
  { style: ButtonStyle.Secondary, label: "2" },
  async (_p: any, index: number, _i: any) => {return (index ?? 0) * 10 + 2}
)
const button3 = new Button(
  async (p, i, d: number) => {await p.update(d, i)},
  { style: ButtonStyle.Secondary, label: "3" },
  async (_p: any, index: number, _i: any) => {return (index ?? 0) * 10 + 3}
)
const button4 = new Button(
  async (p, i, d: number) => {await p.update(d, i)},
  { style: ButtonStyle.Secondary, label: "4" },
  async (_p: any, index: number, _i: any) => {return (index ?? 0) * 10 + 4}
)
const button5 = new Button(
  async (p, i, d: number) => {await p.update(d, i)},
  { style: ButtonStyle.Secondary, label: "5" },
  async (_p: any, index: number, _i: any) => {return (index ?? 0) * 10 + 5}
)
const button6 = new Button(
  async (p, i, d: number) => {await p.update(d, i)},
  { style: ButtonStyle.Secondary, label: "6" },
  async (_p: any, index: number, _i: any) => {return (index ?? 0) * 10 + 6}
)
const button7 = new Button(
  async (p, i, d: number) => {await p.update(d, i)},
  { style: ButtonStyle.Secondary, label: "7" },
  async (_p: any, index: number, _i: any) => {return (index ?? 0) * 10 + 7}
)
const button8 = new Button(
  async (p, i, d: number) => {await p.update(d, i)},
  { style: ButtonStyle.Secondary, label: "8" },
  async (_p: any, index: number, _i: any) => {return (index ?? 0) * 10 + 8}
)
const button9 = new Button(
  async (p, i, d: number) => {await p.update(d, i)},
  { style: ButtonStyle.Secondary, label: "9" },
  async (_p: any, index: number, _i: any) => {return (index ?? 0) * 10 + 9}
)
const button0 = new Button(
  async (p, i, d: number) => {await p.update(d, i)},
  { style: ButtonStyle.Secondary, label: "0" },
  async (_p: any, index: number, _i: any) => {return (index ?? 0) * 10}
)

const numbpad = new Page("numbpad", client)
  .setIsContainer(true)
  .setWindowMax(10)
  .setIsEphemeral(true)
  .addStaticElement(new TextDisplay(async (_p, index, _i) => "# " + index))
  .addStaticElement(new ActionRow([button1, button2, button3]))
  .addStaticElement(new ActionRow([button4, button5, button6]))
  .addStaticElement(new ActionRow([button7, button8, button9]))
  .addStaticElement(new ActionRow([button0]))

export default numbpad;

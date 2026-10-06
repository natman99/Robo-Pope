import { SlashCommandBuilder, ChatInputCommandInteraction } from "discord.js";
import type { Command } from "../deploy";
import numbpad from "../functions/ui_framework/numbpad";

export default {
  data: new SlashCommandBuilder()
      .setName('test')
      .setDescription('test'),
  async execute(interaction: ChatInputCommandInteraction) {
      console.log(interaction.user.username);
      interaction.reply(await numbpad.render(0, interaction))
  },
} as Command;

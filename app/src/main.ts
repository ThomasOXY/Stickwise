import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/inter/700.css";
import "@fontsource/barlow-condensed/500.css";
import "@fontsource/barlow-condensed/600.css";
import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import SettingsWindow from "./SettingsWindow.svelte";

// One frontend, two windows: the Settings window opens with ?window=settings.
const settings = new URLSearchParams(location.search).get("window") === "settings";
mount(settings ? SettingsWindow : App, { target: document.getElementById("app")! });

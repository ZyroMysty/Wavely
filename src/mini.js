import { mount } from "svelte";
import MiniPlayer from "./components/MiniPlayer.svelte";
import "./app.css";

mount(MiniPlayer, { target: document.getElementById("app") });

(() => {
  const rows = [...document.querySelectorAll(".track")];
  if (!rows.length) return;

  const $ = (id) => document.getElementById(id);
  const audio = $("audio");
  const seek = $("seek");
  const PLAY = "M7 4v16l14-8z";
  const PAUSE = "M6 4h4.5v16H6zM13.5 4H18v16h-4.5z";
  let current = -1;
  let seeking = false;

  const clock = (s) => {
    if (!Number.isFinite(s) || s < 0) return "0:00";
    const t = Math.floor(s);
    const h = Math.floor(t / 3600);
    const m = Math.floor((t % 3600) / 60);
    const sec = String(t % 60).padStart(2, "0");
    return h ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
  };

  function load(i, play = true) {
    if (i < 0 || i >= rows.length) return;
    const row = rows[i];
    current = i;
    rows.forEach((r) => {
      r.classList.toggle("on", r === row);
      r.style.removeProperty("--progress");
      r.removeAttribute("aria-current");
    });
    row.setAttribute("aria-current", "true");
    $("player").hidden = false;
    $("play-error").hidden = true;
    $("now-title").textContent = row.dataset.title || row.querySelector(".title").textContent;
    $("now-artist").textContent = row.dataset.artist;
    seek.value = 0;
    seek.style.setProperty("--fill", "0%");
    $("elapsed").textContent = "0:00";
    $("length").textContent = row.querySelector(".time").textContent || "0:00";
    audio.src = row.dataset.src;
    if (play) audio.play().catch(() => {});
    if ("mediaSession" in navigator) {
      navigator.mediaSession.metadata = new MediaMetadata({
        title: $("now-title").textContent,
        artist: row.dataset.artist,
        album: document.querySelector("h1").textContent,
      });
    }
    row.scrollIntoView({ block: "nearest" });
  }

  function toggle() {
    if (current < 0) load(0);
    else if (audio.paused) audio.play().catch(() => {});
    else audio.pause();
  }

  rows.forEach((row, i) => {
    row.addEventListener("click", () => (i === current ? toggle() : load(i)));
  });
  $("toggle").addEventListener("click", toggle);
  $("prev").addEventListener("click", () => {
    if (audio.currentTime > 3) audio.currentTime = 0;
    else load(Math.max(0, current - 1));
  });
  $("next").addEventListener("click", () => load(current + 1));

  const paint = () => {
    const paused = audio.paused;
    $("toggle-icon").setAttribute("d", paused ? PLAY : PAUSE);
    $("toggle").setAttribute("aria-label", paused ? "Přehrát" : "Pozastavit");
    document.body.classList.toggle("playing", !paused);
  };
  audio.addEventListener("play", paint);
  audio.addEventListener("pause", paint);
  audio.addEventListener("ended", () => load(current + 1));
  audio.addEventListener("loadedmetadata", () => {
    $("length").textContent = clock(audio.duration);
  });
  audio.addEventListener("timeupdate", () => {
    if (seeking || !audio.duration) return;
    seek.value = (audio.currentTime / audio.duration) * 1000;
    seek.style.setProperty("--fill", `${seek.value / 10}%`);
    rows[current].style.setProperty("--progress", `${seek.value / 10}%`);
    $("elapsed").textContent = clock(audio.currentTime);
  });
  audio.addEventListener("error", () => {
    if (audio.getAttribute("src")) $("play-error").hidden = false;
  });

  seek.addEventListener("input", () => {
    seeking = true;
    seek.style.setProperty("--fill", `${seek.value / 10}%`);
    if (audio.duration) $("elapsed").textContent = clock((seek.value / 1000) * audio.duration);
  });
  seek.addEventListener("change", () => {
    seeking = false;
    if (audio.duration) audio.currentTime = (seek.value / 1000) * audio.duration;
  });

  document.addEventListener("keydown", (e) => {
    if (e.code !== "Space" || e.target.matches("input, button")) return;
    e.preventDefault();
    toggle();
  });

  if ("mediaSession" in navigator) {
    navigator.mediaSession.setActionHandler("play", toggle);
    navigator.mediaSession.setActionHandler("pause", toggle);
    navigator.mediaSession.setActionHandler("previoustrack", () => load(current - 1));
    navigator.mediaSession.setActionHandler("nexttrack", () => load(current + 1));
  }
})();

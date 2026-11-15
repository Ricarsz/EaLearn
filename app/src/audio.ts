// 全局只留一个 Audio 元素：new Audio(url).play() 的写法里对象没有引用，
// WebKitGTK 上会被 GC 掉，播到一半就停——这是"点了有声音但很短"的常见原因。
let player: HTMLAudioElement | null = null;

export function playUrl(url: string): Promise<void> {
  if (!player) player = new Audio();
  player.pause();
  player.src = url;
  player.currentTime = 0;
  return player.play();
}

export function stopAudio() {
  player?.pause();
}

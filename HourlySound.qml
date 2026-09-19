import QtQuick
import QtMultimedia

Item {
  signal failed(string message)
  function play(percent) {
    if (click.status !== SoundEffect.Ready) { failed("Wooden click is not ready. Check Qt Multimedia and audio output."); return }
    click.volume = Math.max(0, Math.min(1, percent / 100))
    click.play()
  }
  SoundEffect {
    id: click
    source: Qt.resolvedUrl("sounds/wood-click.wav")
    volume: 0.25
  }
}

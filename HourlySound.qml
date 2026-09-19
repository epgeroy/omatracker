import QtQuick
import QtMultimedia

Item {
  id: root
  property alias audioDevice: click.audioDevice
  property bool requested: false
  property bool didStart: false
  readonly property bool ready: click.status === SoundEffect.Ready
  readonly property string outputName: click.audioDevice.description || "system default output"
  signal started(string output, int percent)
  signal finished()
  signal failed(string message)

  function play(percent) {
    if (requested) return
    if (!ready) { failed("Wooden click is not ready. Check Qt Multimedia and audio output."); return }
    if (devices.audioOutputs.length === 0) { failed("No audio output is available"); return }
    click.volume = Math.max(0, Math.min(1, percent / 100))
    requested = true
    didStart = false
    watchdog.restart()
    click.play()
  }

  MediaDevices { id: devices }
  SoundEffect {
    id: click
    // Follow output changes, including headphones connected after shell startup.
    audioDevice: devices.defaultAudioOutput
    source: Qt.resolvedUrl("sounds/wood-click.wav")
    volume: 0.25
    onPlayingChanged: {
      if (!root.requested) return
      if (playing) {
        root.didStart = true
        root.started(root.outputName, Math.round(volume * 100))
      } else if (root.didStart) {
        root.requested = false
        watchdog.stop()
        root.finished()
      }
    }
    onStatusChanged: if (status === SoundEffect.Error) {
      root.requested = false
      watchdog.stop()
      root.failed("Could not load wooden click audio")
    }
  }
  Timer {
    id: watchdog
    interval: 5000
    onTriggered: {
      root.requested = false
      click.stop()
      root.failed("Audio playback did not complete on " + root.outputName)
    }
  }
}

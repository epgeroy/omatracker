import QtQuick
import QtMultimedia
import Quickshell
import "plugin" as App

Item {
  id: root
  property int phase: 0
  property int completed: 0
  MediaDevices { id: devices }
  App.HourlySound {
    id: sound
    onFailed: function(message) { console.error(message); Qt.quit() }
    onFinished: { root.completed++; console.log("Audio completed " + root.completed) }
  }
  Timer {
    interval: 1500
    running: true
    repeat: true
    onTriggered: {
      if (root.phase === 0) {
        var output = devices.audioOutputs.find(function(d) { return d.description === "OmaTrackerAudioTest" })
        if (!output) { console.error("Isolated audio device not found: " + devices.audioOutputs.map(function(d) { return d.description }).join(", ")); Qt.quit(); return }
        // Route only this component into the isolated sink. Never change the
        // user's default output or capture another application's audio.
        sound.audioDevice = output
      } else if (root.phase <= 3) {
        console.log("Audio attempt " + root.phase)
        sound.play(25)
      } else { Qt.quit(); return }
      root.phase++
    }
  }
}

//go:build !linux

package desktop

import "github.com/egoist/mygo"

func notifyImeCursor(w *mygo.Window, x, y, width, height float64) {}

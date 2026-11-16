// Command zoc-fixture is a deterministic OpenCode Server fixture for ZOC's
// integration tests. It is not an OpenCode implementation: it serves only
// the endpoints the official Web UI probes at startup plus an SSE stream.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"strings"
	"sync"
	"time"

	"golang.org/x/net/websocket"
)

func main() {
	addr := flag.String("addr", "127.0.0.1:4099", "listen address")
	logfile := flag.String("log", "", "request log file")
	password := flag.String("password", "", "optional Basic password")
	flag.Parse()

	var mu sync.Mutex
	var logFile *os.File
	if *logfile != "" {
		var err error
		logFile, err = os.OpenFile(*logfile, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600)
		if err != nil {
			log.Fatal(err)
		}
		defer logFile.Close()
	}
	record := func(r *http.Request) {
		if logFile == nil {
			return
		}
		mu.Lock()
		defer mu.Unlock()
		fmt.Fprintf(logFile, "%s %s\n", r.Method, r.URL.Path)
	}

	mux := http.NewServeMux()
	mux.HandleFunc("/api/info", func(w http.ResponseWriter, r *http.Request) {
		record(r)
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]string{"version": "2.3.0-fixture", "hostname": "zoc-fixture"})
	})
	mux.HandleFunc("/api/project", func(w http.ResponseWriter, r *http.Request) {
		record(r)
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte("[]"))
	})
	mux.HandleFunc("/api/session", func(w http.ResponseWriter, r *http.Request) {
		record(r)
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"data":[],"cursor":{"previous":null,"next":null}}`))
	})
	mux.HandleFunc("/api/session/active", func(w http.ResponseWriter, r *http.Request) {
		record(r)
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"data":{}}`))
	})
	mux.Handle("/api/experimental/persistent-pty/pty-fixture/connect", websocket.Server{
		Handshake: func(*websocket.Config, *http.Request) error { return nil },
		Handler: func(ws *websocket.Conn) {
			record(ws.Request())
			websocket.Message.Send(ws, `{"type":"attached","inputProtocol":1,"info":{"size":{"cols":80,"rows":24}},"role":"controller"}`)
			websocket.Message.Send(ws, `{"type":"replay_complete"}`)
			io.Copy(io.Discard, ws)
		},
	})
	mux.HandleFunc("/api/location", func(w http.ResponseWriter, r *http.Request) {
		record(r)
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"directory":"/tmp/zoc-fixture","project":{"id":"fixture","directory":"/tmp/zoc-fixture","canonical":"/tmp/zoc-fixture"}}`))
	})
	mux.HandleFunc("/api/vcs", func(w http.ResponseWriter, r *http.Request) {
		record(r)
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"location":{"directory":"/tmp/zoc-fixture"},"data":{"branch":{"current":"main","default":"main"}}}`))
	})
	mux.HandleFunc("/api/event", func(w http.ResponseWriter, r *http.Request) {
		record(r)
		flusher, ok := w.(http.Flusher)
		if !ok {
			http.Error(w, "streaming unsupported", http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", "text/event-stream")
		w.Header().Set("Cache-Control", "no-cache")
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.WriteHeader(http.StatusOK)
		fmt.Fprint(w, "event: message\ndata: {\"type\":\"server.connected\"}\n\n")
		flusher.Flush()
		ticker := time.NewTicker(10 * time.Second)
		defer ticker.Stop()
		for {
			select {
			case <-r.Context().Done():
				return
			case <-ticker.C:
				fmt.Fprint(w, ": keepalive\n\n")
				flusher.Flush()
			}
		}
	})
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		record(r)
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.Header().Set("Access-Control-Allow-Headers", "*")
		w.Header().Set("Access-Control-Allow-Methods", "*")
		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		http.NotFound(w, r)
	})

	handler := http.Handler(mux)
	if *password != "" {
		handler = http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			user, pass, ok := r.BasicAuth()
			if !ok || user != "opencode" || pass != *password {
				w.WriteHeader(http.StatusUnauthorized)
				return
			}
			mux.ServeHTTP(w, r)
		})
	}
	server := &http.Server{Addr: *addr, Handler: handler}
	log.Printf("zoc fixture listening on http://%s", *addr)
	if err := server.ListenAndServe(); err != nil && !strings.Contains(err.Error(), "Server closed") {
		log.Fatal(err)
	}
}

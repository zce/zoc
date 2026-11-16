package main

import (
	"net/http"

	"github.com/egoist/mygo/netproxy"
	"github.com/zce/zoc/internal/networkpolicy"
)

// httpClient builds Go-side HTTP clients with the system proxy settings and
// the ZOC origin/redirect policy.
type httpClient struct {
	policy *networkpolicy.Policy
}

func (c *httpClient) httpClient() *http.Client {
	return &http.Client{
		Transport:     netproxy.Transport(),
		CheckRedirect: c.policy.CheckRedirect,
		Timeout:       0,
	}
}

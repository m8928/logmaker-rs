{{/*
LOGMAKER_PEERS: each peer release name becomes name=http://<its Service>:<port>.
*/}}
{{- define "logmaker.peers" -}}
{{- $entries := list -}}
{{- range .Values.peers -}}
{{- if contains "=" . -}}
{{- $entries = append $entries . -}}
{{- else -}}
{{- $entries = append $entries (printf "%s=http://%s:%v" . . $.Values.service.port) -}}
{{- end -}}
{{- end -}}
{{- join "," $entries -}}
{{- end -}}

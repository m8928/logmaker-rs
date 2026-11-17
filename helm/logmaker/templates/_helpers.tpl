{{/*
Names of the instances, as a JSON array. `instances` is a count (the first
instance is named after the release, the others <release>-2, <release>-3, ...)
or a list of names. Each instance gets a Deployment, a Service and volumes
named after it.
*/}}
{{- define "logmaker.instances" -}}
{{- $instances := .Values.instances -}}
{{- if kindIs "invalid" $instances -}}
{{- /* Missing after `helm upgrade --reuse-values` from a chart without it. */ -}}
{{- $instances = 1 -}}
{{- end -}}
{{- $names := list -}}
{{- if kindIs "slice" $instances -}}
{{- range $instances -}}
{{- $names = append $names (toString .) -}}
{{- end -}}
{{- else -}}
{{- $count := int $instances -}}
{{- if lt $count 1 -}}
{{- fail "instances must be at least 1" -}}
{{- end -}}
{{- range $i := until $count -}}
{{- $names = append $names (ternary $.Release.Name (printf "%s-%d" $.Release.Name (add1 $i)) (eq $i 0)) -}}
{{- end -}}
{{- end -}}
{{- if not $names -}}
{{- fail "instances must not be empty" -}}
{{- end -}}
{{- range $i, $name := $names -}}
{{- if not (regexMatch "^[a-z0-9]([-a-z0-9]{0,53}[a-z0-9])?$" $name) -}}
{{- fail (printf "invalid instance name %q: use lowercase letters, digits and '-' (at most 55)" $name) -}}
{{- end -}}
{{- if and (eq $name $.Release.Name) (ne $i 0) -}}
{{- fail (printf "instance %s is named after the release, so it must come first" $name) -}}
{{- end -}}
{{- end -}}
{{- if ne (len $names) (len (uniq $names)) -}}
{{- fail "instance names must be unique" -}}
{{- end -}}
{{- toJson $names -}}
{{- end -}}

{{/*
LOGMAKER_PEERS: the instances of this release (name=http://<its Service>:<port>),
then the extra `peers` that are not among them.
*/}}
{{- define "logmaker.peers" -}}
{{- $names := include "logmaker.instances" . | fromJsonArray -}}
{{- $entries := list -}}
{{- range $names -}}
{{- $entries = append $entries (printf "%s=http://%s:%v" . . $.Values.service.port) -}}
{{- end -}}
{{- range .Values.peers -}}
{{- $name := splitList "=" . | first -}}
{{- if not (has $name $names) -}}
{{- if contains "=" . -}}
{{- $entries = append $entries . -}}
{{- else -}}
{{- $entries = append $entries (printf "%s=http://%s:%v" . . $.Values.service.port) -}}
{{- end -}}
{{- end -}}
{{- end -}}
{{- join "," $entries -}}
{{- end -}}

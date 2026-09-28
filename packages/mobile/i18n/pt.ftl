# European Portuguese; also serves pt-BR. Missing keys fall back to en.ftl.

## Shared
start = Iniciar
stop = Parar
cancel = Cancelar
open-settings = Abrir definições
try-again = Tentar de novo
finishing = A terminar…
stat-top = Máx.
stat-avg = Média
stat-distance = Distância
stat-time = Tempo
pb = RP
no-completed-laps = Nenhuma volta completa
sector-count = { $count ->
    [one] 1 setor
   *[other] { $count } setores
}

## Tabs
tab-history = Histórico
tab-tracks = Pistas
tab-speed = Velocidade
tab-editor = Editor
tab-settings = Definições
back-to-live = A gravar · voltar ao direto

## GPS
gps-off = GPS desligado
gps-connecting = GPS a ligar…
gps-no-fix = Sem sinal GPS
gps-fix = Sinal GPS · { $source } · { $satellites ->
    [one] 1 satélite
   *[other] { $satellites } satélites
} · ±{ $accuracy }
gps-permission-denied = Permissão de localização recusada. É necessária para gravar.
gps-location-disabled = A localização está desligada. Ligue-a para gravar.
gps-unavailable = O GPS não está disponível: { $detail }
gps-no-device = Nenhum dispositivo GPS encontrado.
permission-ask = A permissão de localização é pedida ao Iniciar
permission-granted = Permissão de localização concedida
permission-blocked = A permissão de localização está bloqueada. Permita-a nas Definições e inicie de novo.

## Tracks and speedometer
tracks = Pistas
no-tracks = Ainda não há pistas.
import-a-track = Importar uma pista
edit-track = Editar pista
speedometer = Velocímetro
speedometer-idle = { $unit } · máxima, média e distância, sem pista
developer = Programador
mock-drive = Condução simulada em vez do GPS
force-location-manager = LocationManager em vez do Fused

## Live
not-recording = Sem gravação.
new-recording = Nova gravação
waiting-for-start = À espera da linha de partida…
lap-n = Volta { $n }
sector-n = Setor { $n }
finished = Terminado
last-sector = Último setor { $time }
last-lap = Última volta { $time }
best = Melhor { $time }
delta = Delta
gps-ended = O sinal GPS terminou. Pare para ver os resultados.
g-calibrating = A calibrar…

## Results and history
results = Resultados
recording-in-progress = Gravação em curso.
personal-best = Recorde pessoal nesta pista
splits = Parciais
gps-gap = Falha de GPS nesta volta
g-braking = Travagem máx.
g-accelerating = Aceleração máx.
g-lateral = Lateral máx. (E · D)
g-lap = Trav. · Lat. g
history = Histórico
no-recordings = Ainda não há gravações.
recording-not-found = Gravação não encontrada.
interrupted = Interrompida
interrupted-detail = Interrompida: a app fechou a meio da gravação, guardada até à última posição.
no-track = Sem pista
speedometer-only = Só velocímetro
outcome-laps = { $count ->
    [one] 1 volta
   *[other] { $count } voltas
} · melhor { $best }
outcome-run = Percurso { $best }

## Editor
editor = Editor
create-track = Criar nova pista
record-track = Gravar uma pista
import-file = Importar ficheiro CSV, GPX ou KML
edit-saved-track = Editar uma pista guardada
track-not-found = Pista não encontrada.
recorded-track = Pista gravada
record-hint = Conduza ou caminhe pela pista. Pare no fim, ou de volta à partida se for um circuito.
starting-gps = A iniciar o GPS…
mode-draw = Desenhar
mode-gate = Linha
mode-start = Partida
mode-finish = Chegada
mode-sectors = Setores
hint-selected = Arraste para o mover, ou apague-o.
hint-draw = Toque no mapa para adicionar pontos. Arraste um para o mover, ou arraste um vazio para o inserir. Toque num para o apagar.
hint-sectors = Toque nos pontos para adicionar ou remover linhas de setor.
hint-gate = Toque num ponto para mover a linha para lá.
undo = Anular
delete-point = Apagar ponto
name = Nome
closed-loop = Circuito fechado, cronometrado por voltas
live-track-locked = Pare a gravação nesta pista para mover as linhas ou apagá-la.
save-track = Guardar pista
discard = Descartar
delete-track = Apagar pista
keep-track = Manter pista
delete-warning = { $count ->
    [0] Apaga a pista.
    [one] Apaga a pista e a sua gravação.
   *[other] Apaga a pista e as suas { $count } gravações.
}
save-failed = Não foi possível guardar a pista.
delete-failed = Não foi possível apagar a pista.

## Track errors
too-few-waypoints = Toque no mapa para adicionar pelo menos dois pontos.
placement-off-track = Uma linha está a { $distance } da pista, longe demais para a colocar.
placement-beyond-track = Uma linha está { $distance } além do fim da pista. Estenda os pontos até ela.
finish-before-start = A chegada tem de vir depois da partida ao longo dos pontos.
sector-outside = Uma linha de setor não está entre a partida e a chegada.
import-unrecognised = Não é um CSV com colunas de latitude e longitude, um GPX ou um KML.
import-bad-line = A linha { $line } não tem latitude e longitude válidas.
import-bad-xml = XML inválido: { $detail }

## Map
finding-location = A procurar a sua localização…
location-not-found = Não foi possível encontrar a sua localização.
no-places = Nenhum local encontrado.
search-failed = A pesquisa falhou: { $error }
search-place = Procurar um local
layer-streets = Ruas
layer-satellite = Satélite
my-location = A minha localização
fit-track = Ajustar à pista

## Settings
settings = Definições
units = Unidades
units-metric = Métrico
units-imperial = Imperial
theme = Tema
theme-system = Sistema
theme-light = Claro
theme-dark = Escuro
language = Idioma
language-system = Sistema
language-en = English
language-pt = Português

## Backup
backup = Cópia de segurança
backup-note = Tudo fica apenas neste telemóvel. Exporte para guardar uma cópia.
export-backup = Exportar cópia
restore-warning = Restaurar substitui todas as pistas e gravações aqui pelas da cópia.
restore-choose = Escolher cópia e substituir
restore-backup = Restaurar cópia
backup-saved = Cópia guardada.
backup-restored = Cópia restaurada.
backup-failed = A cópia falhou: { $error }

back-to-replay = A reproduzir · voltar à reprodução
replay = Reproduzir
end-replay = Terminar reprodução
replay-ended = Reprodução terminada.
replay-trim = Saltar antes da partida e depois da chegada

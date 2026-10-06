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

## Background guide
guide-title = Continuar a gravar com o ecrã desligado
guide-why = O gestor de bateria do telemóvel pode fechar o Traced com o ecrã desligado, terminando a gravação. Permita que funcione em segundo plano:
guide-killed-title = A última gravação foi interrompida
guide-killed = O gestor de bateria do telemóvel fechou o Traced durante a gravação. Permita que funcione em segundo plano para não voltar a acontecer:
guide-step-settings = Toque em Abrir definições.
guide-step-battery = Abra Utilização da bateria (ou Bateria).
guide-step-allow = Escolha Permitir atividade em segundo plano (nalguns telemóveis, Sem restrições).
guide-done = Concluído
guide-not-yet = O Traced ainda não tem permissão para funcionar em segundo plano. Siga os passos e toque em Concluído.
guide-skip = Agora não

## Tracks and speedometer
tracks = Pistas
no-tracks = Ainda não há pistas.
add-track = Adicionar uma pista
no-tracks-body = Importe um ficheiro de pista, desenhe-a no mapa ou grave-a dando uma volta.
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
braking-zones = Zonas de travagem · volta { $lap }
no-braking-zones = Sem travagens nesta volta.
braking-at = Aos
braking-speeds = Entrada → saída
braking-peak = Pico
braking-vs-pb = vs RP
trace-lap = Volta { $lap }
trace-hint = Toque no gráfico para ler as velocidades.
history = Histórico
no-recordings = Ainda não há gravações.
no-recordings-body = As suas sessões aparecem aqui depois de gravar uma.
choose-track = Escolher uma pista
recording-not-found = Gravação não encontrada.
delete-recording = Apagar gravação
keep-recording = Manter gravação
delete-recording-warning = Apaga esta gravação de vez.
delete-recording-failed = Não foi possível apagar a gravação.
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
reverse = Inverter
undo = Anular
delete-point = Apagar ponto
name = Nome
tags = Etiquetas
tags-hint = Adicionar etiqueta, ex. karting
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
live-map = Mapa ao vivo
live-map-follow = Seguir-me
live-map-overview = Pista inteira

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

## Updates
updates = Atualizações
update-version = Traced { $version }
update-checking = A procurar uma versão mais recente…
update-none = Esta é a versão mais recente.
update-check-failed = Não foi possível procurar atualizações: { $error }
update-install = Instalar a versão { $version }
update-whats-new = Novidades na versão { $version }
update-downloading = A transferir…
no-internet = Sem ligação à internet
update-failed = A atualização falhou: { $error }
update-available = A versão { $version } está disponível · atualizar

back-to-replay = A reproduzir · voltar à reprodução
replay = Reproduzir
end-replay = Terminar reprodução
replay-ended = Reprodução terminada.
replay-trim = Saltar antes da partida e depois da chegada

## Partilhar
share = Partilhar
share-track = Partilhar pista
share-with-best = Mostrar a minha melhor volta
share-with-gpx = Anexar o ficheiro da pista (GPX)
save-image = Guardar imagem
save-gpx = Guardar ficheiro da pista (GPX)
share-saved = Guardado.
share-failed = Não foi possível partilhar: { $error }
card-length = Extensão
card-sectors = Setores
card-best-lap = Melhor volta

## Onboarding
skip = Saltar
next = Seguinte
continue = Continuar
get-started = Começar
welcome-speed = Velocímetro, em qualquer lado
welcome-speed-body = Velocidade máxima e média, distância e forças G. Sem pista.
welcome-laps = Cronómetro de voltas
welcome-laps-body = Escolha uma pista e toque em Iniciar. Cada volta e setor é cronometrado, em direto contra a sua melhor.
welcome-editor = Qualquer pista, à sua maneira
welcome-editor-body = Desenhe uma pista no mapa, grave-a dando uma volta ou importe um ficheiro GPX, KML ou CSV.
location-title = O Traced precisa da sua localização
location-laps = É pelo GPS que o Traced sabe quando cruza a linha, para cronometrar as voltas.
location-recording = Só é usada enquanto grava.
location-screen-off = A gravação continua com o ecrã desligado, para poder guardar o telemóvel.
notifications-title = Saiba que continua a gravar
notifications-recording = Enquanto grava, uma notificação silenciosa mostra que o Traced continua a cronometrar com o ecrã desligado.
notifications-back = Toque nela para voltar à sua sessão.
tutorial = Tutorial
tutorial-replay = Mostrar o tutorial de novo

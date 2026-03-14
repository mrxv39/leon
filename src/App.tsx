import { ChangeEvent, DragEvent, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type MrzData = {
  documentNumber: string;
  surname: string;
  givenNames: string;
  nationality: string;
  birthDate: string;
  sex: string;
  expiryDate: string;
};

type ExtractedDocument = {
  rawOcr: string;
  mrz?: MrzData;
  fields: Record<string, string>;
  warnings?: string[];
};

const FIELD_LABELS: Record<string, string> = {
  documentNumber: "Numero de documento",
  surname: "Apellidos",
  givenNames: "Nombre",
  nationality: "Nacionalidad",
  birthDate: "Fecha de nacimiento",
  sex: "Sexo",
  expiryDate: "Fecha de caducidad",
  issuingCountry: "Pais emisor",
  documentType: "Tipo de documento",
  fullName: "Nombre completo",
};

const PROGRESS_MESSAGES = [
  "Analizando documento...",
  "Detectando MRZ...",
  "Extrayendo campos...",
];
const DEFAULT_ESTIMATED_DURATION = 60;

function fileToDataUrl(file: File) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ""));
    reader.onerror = () => reject(new Error("No se pudo leer el archivo."));
    reader.readAsDataURL(file);
  });
}

function normalizeFields(result?: ExtractedDocument) {
  if (!result) {
    return {};
  }

  return {
    ...result.fields,
    ...(result.mrz ?? {}),
  };
}

function formatElapsedTime(totalSeconds: number) {
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

function App() {
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [selectedName, setSelectedName] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [result, setResult] = useState<ExtractedDocument | null>(null);
  const [editableFields, setEditableFields] = useState<Record<string, string>>({});
  const [showStartToast, setShowStartToast] = useState(false);
  const [progressValue, setProgressValue] = useState(0);
  const [progressMessageIndex, setProgressMessageIndex] = useState(0);
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const [estimatedTotalSeconds, setEstimatedTotalSeconds] = useState(
    DEFAULT_ESTIMATED_DURATION,
  );
  const toastTimeoutRef = useRef<number | null>(null);
  const startTimeRef = useRef<number | null>(null);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }

    const savedDuration = Number(window.sessionStorage.getItem("leon_last_duration"));
    if (Number.isFinite(savedDuration) && savedDuration > 0) {
      setEstimatedTotalSeconds(savedDuration);
    }
  }, []);

  const orderedFields = useMemo(() => {
    const preferredOrder = [
      "documentType",
      "documentNumber",
      "surname",
      "givenNames",
      "fullName",
      "nationality",
      "issuingCountry",
      "birthDate",
      "sex",
      "expiryDate",
    ];

    const known = preferredOrder.filter((key) => key in editableFields);
    const custom = Object.keys(editableFields)
      .filter((key) => !preferredOrder.includes(key))
      .sort((a, b) => a.localeCompare(b));

    return [...known, ...custom];
  }, [editableFields]);

  useEffect(() => {
    if (!loading) {
      setProgressValue(0);
      setProgressMessageIndex(0);
      setElapsedSeconds(0);
      return;
    }

    setProgressValue(6);
    setProgressMessageIndex(0);
    setElapsedSeconds(0);

    const progressTimer = window.setInterval(() => {
      setProgressValue((current) => {
        if (current >= 85) {
          return current;
        }

        const next = current + (current < 40 ? 6 : current < 65 ? 4 : 2);
        return Math.min(next, 85);
      });
    }, 1600);

    const messageTimer = window.setInterval(() => {
      setProgressMessageIndex(
        (current) => (current + 1) % PROGRESS_MESSAGES.length,
      );
    }, 2400);

    const elapsedTimer = window.setInterval(() => {
      if (startTimeRef.current === null) {
        return;
      }

      setElapsedSeconds(
        Math.floor((Date.now() - startTimeRef.current) / 1000),
      );
    }, 1000);

    return () => {
      window.clearInterval(progressTimer);
      window.clearInterval(messageTimer);
      window.clearInterval(elapsedTimer);
    };
  }, [loading]);

  useEffect(() => {
    return () => {
      if (toastTimeoutRef.current !== null) {
        window.clearTimeout(toastTimeoutRef.current);
      }
    };
  }, []);

  function applyResult(nextResult: ExtractedDocument) {
    setResult(nextResult);
    setEditableFields(normalizeFields(nextResult));
  }

  function onFileSelected(file: File | null) {
    setSelectedFile(file);
    setSelectedName(file?.name ?? "");
    setError("");
  }

  function onInputFile(event: ChangeEvent<HTMLInputElement>) {
    onFileSelected(event.currentTarget.files?.[0] ?? null);
  }

  function onDrop(event: DragEvent<HTMLLabelElement>) {
    event.preventDefault();
    onFileSelected(event.dataTransfer.files?.[0] ?? null);
  }

  function onDragOver(event: DragEvent<HTMLLabelElement>) {
    event.preventDefault();
  }

  async function extractDocument() {
    if (!selectedFile) {
      setError("Selecciona una imagen de pasaporte o DNI antes de extraer.");
      return;
    }

    if (toastTimeoutRef.current !== null) {
      window.clearTimeout(toastTimeoutRef.current);
    }

    setShowStartToast(true);
    setLoading(true);
    setProgressValue(0);
    setProgressMessageIndex(0);
    setElapsedSeconds(0);
    setError("");
    startTimeRef.current = Date.now();
    toastTimeoutRef.current = window.setTimeout(() => {
      setShowStartToast(false);
      toastTimeoutRef.current = null;
    }, 1600);

    try {
      const imageBase64 = await fileToDataUrl(selectedFile);
      const extraction = await invoke<ExtractedDocument>("extract_document", {
        imageBase64,
      });
      setProgressValue(100);
      if (startTimeRef.current !== null) {
        const totalElapsed = Math.max(
          1,
          Math.floor((Date.now() - startTimeRef.current) / 1000),
        );
        setElapsedSeconds(totalElapsed);
        setEstimatedTotalSeconds(totalElapsed);
        window.sessionStorage.setItem("leon_last_duration", String(totalElapsed));
      }
      applyResult(extraction);
    } catch (invokeError) {
      const message =
        invokeError instanceof Error
          ? invokeError.message
          : "No se pudo extraer el documento.";
      setError(message);
    } finally {
      if (toastTimeoutRef.current !== null) {
        window.clearTimeout(toastTimeoutRef.current);
        toastTimeoutRef.current = null;
      }
      setShowStartToast(false);
      setLoading(false);
      startTimeRef.current = null;
    }
  }

  return (
    <main className="app-shell">
      <div className={`start-toast ${showStartToast ? "visible" : ""}`}>
        <span className="start-toast-icon" aria-hidden="true">
          scan
        </span>
        <div>
          <strong>Iniciando escaneo...</strong>
          <p>Preparando el analisis del documento.</p>
        </div>
      </div>

      <div className={`scan-overlay ${loading ? "visible" : ""}`} aria-hidden={!loading}>
        <div className="scan-card" role="status" aria-live="polite">
          <div className="scan-badge">Escaneo activo</div>
          <h2>{PROGRESS_MESSAGES[progressMessageIndex]}</h2>
          <p>
            El OCR y la lectura MRZ se estan ejecutando en segundo plano. La ventana seguira
            respondiendo mientras termina el analisis.
          </p>

          <div className="progress-track">
            <div
              className="progress-fill"
              style={{ width: `${progressValue}%` }}
            />
          </div>

          <div className="scan-meta">
            <span>{Math.round(progressValue)}%</span>
            <span className="scan-pulse">Procesando</span>
          </div>

          <div className="scan-timing">
            <span>Tiempo transcurrido: {formatElapsedTime(elapsedSeconds)}</span>
            <span>
              Restante: ~
              {Math.max(0, estimatedTotalSeconds - elapsedSeconds)} s
            </span>
          </div>
        </div>
      </div>

      <section className="hero-card">
        <div className="hero-copy">
          <p className="eyebrow">LE⏻N</p>
          <h1>Reconocimiento de documentos</h1>
          <p className="hero-text">
            Carga una imagen de pasaporte o documento de identidad para ejecutar
            OCR, detectar MRZ y editar los campos extraidos.
          </p>
        </div>

        <div className="upload-panel">
          <label
            className="dropzone"
            onDrop={onDrop}
            onDragOver={onDragOver}
          >
            <input
              type="file"
              accept="image/png,image/jpeg,image/webp,image/bmp"
              onChange={onInputFile}
            />
            <span className="dropzone-title">
              Arrastra una imagen aqui o selecciona un archivo
            </span>
            <span className="dropzone-subtitle">
              Formatos recomendados: PNG, JPG, WEBP o BMP.
            </span>
            {selectedName ? (
              <span className="selected-file">{selectedName}</span>
            ) : null}
          </label>

          <button
            className="extract-button"
            onClick={extractDocument}
            disabled={!selectedFile || loading}
            type="button"
          >
            {loading ? "Extrayendo..." : "Extraer"}
          </button>
        </div>
      </section>

      {error ? <p className="error-banner">{error}</p> : null}

      <section className="results-grid">
        <article className="panel">
          <div className="panel-heading">
            <h2>Campos editables</h2>
            <p>Corrige o completa la informacion detectada.</p>
          </div>

          {result?.warnings && result.warnings.length > 0 ? (
            <div className="warning-banner" role="alert">
              <div className="warning-icon" aria-hidden="true">
                !
              </div>
              <div className="warning-copy">
                <strong>Revisa estos valores antes de continuar</strong>
                {result.warnings.map((warning) => (
                  <p key={warning}>{warning}</p>
                ))}
              </div>
            </div>
          ) : null}

          {orderedFields.length === 0 ? (
            <p className="empty-state">
              Los campos apareceran aqui despues de ejecutar la extraccion.
            </p>
          ) : (
            <div className="fields-grid">
              {orderedFields.map((fieldKey) => (
                <label className="field" key={fieldKey}>
                  <span>{FIELD_LABELS[fieldKey] ?? fieldKey}</span>
                  <input
                    type="text"
                    value={editableFields[fieldKey] ?? ""}
                    onChange={(event) =>
                      setEditableFields((current) => ({
                        ...current,
                        [fieldKey]: event.currentTarget.value,
                      }))
                    }
                  />
                </label>
              ))}
            </div>
          )}
        </article>

        <article className="panel">
          <div className="panel-heading">
            <h2>JSON devuelto</h2>
            <p>Respuesta completa del comando Tauri.</p>
          </div>
          <pre className="json-output">
            {result ? JSON.stringify(result, null, 2) : "Sin resultados todavia."}
          </pre>
        </article>

        <article className="panel">
          <div className="panel-heading">
            <h2>Texto OCR</h2>
            <p>Salida en bruto para depurar el reconocimiento.</p>
          </div>
          <pre className="ocr-output">{result?.rawOcr ?? "Sin OCR todavia."}</pre>
        </article>
      </section>
    </main>
  );
}

export default App;

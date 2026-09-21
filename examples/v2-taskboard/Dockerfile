ARG PYTHON_IMAGE=python:3.12-slim@sha256:78387bc3881b8273120a12ebe6c1ab22b018ccc2c9adf565ae1ac9b536e184ea
FROM ${PYTHON_IMAGE}
ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1 HOST=0.0.0.0 PORT=8080
WORKDIR /app
COPY taskboard ./taskboard
COPY checks ./checks
COPY tests ./tests
COPY e2e ./e2e
COPY scripts ./scripts
USER 65532:65532
EXPOSE 8080
CMD ["python", "-m", "taskboard.server"]

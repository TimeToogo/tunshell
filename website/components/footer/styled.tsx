import styled from "styled-components";
import { COLOURS } from "../../theme/colours";

export const Footer = styled.header`
  padding: 20px 0;
  margin-top: auto;

  &,
  a {
    color: ${COLOURS.OFF_WHITE};
  }
`;

export const Contents = styled.div`
  display: flex;
  flex-direction: row;
  justify-content: center;
`;

export const Credits = styled.div`
  display: flex;
  flex-direction: row;
  align-items: center;

  ion-icon {
    margin-left: 5px;
  }

  @media (max-width: 900px) {
    display: block;

    ion-icon {
      margin-top: 5px;
      margin-left: 0;
    }
  }
`;
